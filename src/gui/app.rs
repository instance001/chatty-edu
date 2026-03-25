use crate::chat::{generate_answer, generate_answer_with_system_prompt};
use crate::ecg_window::EcgWindowState;
use crate::homework_markdown::{self, PackMdDefaults};
use crate::homework_pack::{
    apply_pack_policy, create_pack_multi, export_pack_template, find_latest_pack,
    load_pack_from_file, load_submission_summaries, save_submission_for_assignment,
    HomeworkAssignment, HomeworkPack, SubmissionSummary,
};
use crate::local_model;
use crate::memory::{bookkeeper_dir, load_memory_jogger, ColdLogEntry, EduBookkeeperHandle};
use crate::model_registry::{discover_gguf_models, gguf_magic_ok};
use crate::modules::{load_modules, role_allowed, LoadedModule, ModuleEntry};
use crate::revision::{
    build_revision_pack_markdown, import_past_paper, load_past_papers, load_revision_progress,
    load_revision_sources, revision_dir, revision_past_papers_dir, revision_priority,
    save_revision_progress, RevisionProgress, RevisionSource,
};
use crate::settings::{save_settings, Settings};
use crate::theme::{
    apply_theme, ensure_theme_files, load_presets, load_theme, save_theme, ThemeConfig,
};
use chrono::Utc;
use deunicode::deunicode;
use eframe::{
    egui::{
        self, menu, scroll_area::ScrollBarVisibility, Align, CentralPanel, Context, Layout,
        ProgressBar, RichText, ScrollArea, TopBottomPanel,
    },
    App, CreationContext,
};
use rfd::FileDialog;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::panic;
use std::path::{Path, PathBuf};
use std::time::Instant;

const CHAT_CAPSULE: &str = "Chatty-EDU - Chat Capsule (Chat tab system prompt)\n\
Role: You are Chatty-EDU, an offline learning assistant running entirely on a local computer. You do not have internet access and never browse, search, or fetch links.\n\
Scope: Help with learning questions, explanations, and clarification. Keep responses short, clear, and factual. Default to one concise response unless the user asks for more detail.\n\
Style: Do not invent conversations, roles, or dialogue. Do not hallucinate prior context or role-play multiple speakers. Avoid rambling, repetition, or motivational speeches.\n\
Safety: Use school-appropriate language. If something is outside scope or inappropriate, give one calm sentence that you cannot help and suggest a safe alternative.\n\
Defaults: If you are unsure what the user wants, ask one short clarifying question. If asked what you can do, briefly explain your learning-help role.\n";

const REVISION_CHAT_CAPSULE: &str = "Chatty-EDU - Revision Helper Capsule\n\
Role: You are Chatty-EDU Revision Helper. The student is revising work that has already been submitted.\n\
Scope: Help the student understand past work, revisit mistakes, explain concepts, practice similar questions, and build confidence.\n\
Style: You may explain more openly than the live homework helper because the assignment has already been submitted, but still prefer teaching, reasoning, and mini-examples over blunt answer dumping.\n\
Format: Keep replies short, clear, and useful. A brief explanation, short worked idea, checklist, or quiz question is good.\n\
Social cues: If the student only says a short social cue like \"thanks\", \"thank you\", \"got it\", or \"bye\", reply with one short warm acknowledgement and do not restart the lesson.\n\
Privacy: Never mention internal revision priority, AI scores, saved confidence numbers, or diagnostic feedback labels to the student. Use background signals silently.\n\
Tone: Neutral, supportive, and practical.\n";

const TEACHER_PACK_CAPSULE: &str = "Chatty-EDU - Teacher Homework Pack Generator Capsule\n\
Role: You draft homework packs for teachers. Output is a single Markdown file that will be transcribed to JSON by Chatty-EDU.\n\
Offline: This runs entirely offline. Do not reference web links or browsing.\n\
Output rules:\n\
- Output ONLY the Markdown file contents. No preamble, no explanation, no code fences.\n\
- Pack metadata (optional, near the top): version: 1.0, school_id: <id>, class_id: <id>, created_at: <RFC3339>.\n\
- Each assignment MUST start with: \"## Assignment: <id> | <title>\" (use unique ids like hw-001, hw-002).\n\
- After the assignment heading, include metadata lines as needed:\n\
  subject: <Subject>\n\
  year_level: <Year or Grade>\n\
  due_at: <RFC3339 or blank>\n\
  allow_games: false\n\
  allow_ai_premark: true\n\
  max_score: <int>\n\
  attachments:\n\
  - <path>\n\
- Use `year_level` as the canonical key. Chatty-EDU also accepts older variants like `year`, `year level`, `grade`, and `grade level` when importing or transcribing packs.\n\
- Then include a heading: \"### Instructions\" followed by the questions/tasks in Markdown.\n\
- Optional sections (per assignment):\n\
  - \"### Student Printable\" (paper-friendly student handout; defaults to Instructions if omitted)\n\
  - \"### Rubric\" or \"### Marking Guide\" (teacher marking guide)\n\
Quality:\n\
- Keep it clear, age-appropriate, and easy to complete.\n\
- Prefer a short list of tasks/questions.\n";

#[derive(Debug, Clone, Default)]
struct AssignmentDraft {
    id: String,
    title: String,
    subject: String,
    year_level: String,
    due_at: String,
    instructions_md: String,
    allow_games: bool,
    allow_ai_premark: bool,
    max_score: String,
}

#[derive(Debug, Clone)]
struct StudentScore {
    #[allow(dead_code)]
    student_id: String,
    student_name: String,
    subject: String,
    score: f32, // 0-100
}

#[derive(Debug, Clone)]
struct SubmissionRow {
    #[allow(dead_code)]
    assignment_id: String,
    assignment_title: String,
    student_id: String,
    student_name: String,
    subject: String,
    score: String,
    feedback: String,
    #[allow(dead_code)]
    submitted_at: String,
}

#[derive(Debug, Clone)]
enum TabKind {
    Home,
    Chat,
    Bookkeeper,
    Settings,
    Diagnostics,
    Module {
        module: LoadedModule,
        cached_text: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct Tab {
    id: usize,
    title: String,
    kind: TabKind,
    closable: bool,
    key: String,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum DiagnosticsAudience {
    StudentSafe,
    Teacher,
}

#[derive(Debug, Clone)]
struct LocalModelFile {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct HomeworkQuestionIntercept {
    assignment_id: String,
    question_number: Option<usize>,
    question_text: String,
    normalized_question: String,
    tokens: Vec<String>,
    keyword_tokens: Vec<String>,
    number_tokens: Vec<String>,
    signature_phrases: Vec<String>,
}

fn discover_local_models(base: &Path) -> Vec<LocalModelFile> {
    discover_gguf_models(base)
        .into_iter()
        .map(|model| LocalModelFile {
            name: model.name,
            path: model.path,
        })
        .collect()
}

pub struct ChattyApp {
    pub settings: Settings,
    base_path: PathBuf,
    modules: Vec<LoadedModule>,
    tabs: Vec<Tab>,
    active_tab: usize,
    next_tab_id: usize,
    chat_input: String,
    chat_log: Vec<(String, String)>,
    memory_jogger: String,
    bookkeeper: Option<EduBookkeeperHandle>,
    bookkeeper_query: String,
    bookkeeper_results: Vec<ColdLogEntry>,
    bookkeeper_status: Option<String>,
    theme: ThemeConfig,
    ecg_window: EcgWindowState,
    presets: Vec<ThemeConfig>,
    allow_external_process: bool,
    current_pack: Option<HomeworkPack>,
    submissions: Vec<SubmissionSummary>,
    selected_assignment: Option<String>,
    submission_text: String,
    draft_assignments: Vec<HomeworkAssignment>,
    draft_input: AssignmentDraft,
    selected_students: HashSet<String>,
    submission_attachments: Vec<String>,
    available_models: Vec<LocalModelFile>,
    teacher_unlocked: bool,
    teacher_pin_input: String,
    teacher_pin_new: String,
    teacher_pin_confirm: String,
    teacher_pin_status: Option<String>,
    teacher_secret_answer_input: String,
    teacher_secret_question_input: String,
    revision_sources: Vec<RevisionSource>,
    revision_progress: HashMap<String, RevisionProgress>,
    selected_revision: Option<String>,
    revision_notes: String,
    revision_confidence: i32,
    revision_status: Option<String>,
    revision_chat_input: String,
    revision_chat_log: Vec<(String, String)>,
    past_papers: Vec<PathBuf>,
    teacher_pack_request: String,
    teacher_pack_markdown: String,
    teacher_pack_status: Option<String>,
    teacher_tools_status: Option<String>,
    teacher_revision_status: Option<String>,
    diagnostics_report: String,
    diagnostics_audience: DiagnosticsAudience,
    diagnostics_status: Option<String>,
    homework_question_index: Vec<HomeworkQuestionIntercept>,
}

impl ChattyApp {
    pub fn new(
        cc: &CreationContext<'_>,
        base_path: PathBuf,
        settings: Settings,
    ) -> io::Result<Self> {
        ensure_theme_files(&base_path)?;
        let presets = load_presets(&base_path);
        let theme = load_theme(&base_path, settings.ui.last_theme.as_deref());
        apply_theme(&theme, &cc.egui_ctx);

        let modules = load_modules(&base_path).unwrap_or_default();
        let models = discover_local_models(&base_path);
        let pack = find_latest_pack(&base_path)
            .ok()
            .flatten()
            .map(|(_p, pack)| pack);
        let memory_jogger = load_memory_jogger(&base_path);
        let bookkeeper = EduBookkeeperHandle::start(&base_path);
        let submissions = load_submission_summaries(&base_path).unwrap_or_default();
        let initial_selected = pack.as_ref().and_then(|p| {
            Self::unique_assignments_by_id(p)
                .first()
                .map(|a| a.id.clone())
        });
        let teacher_secret_question = settings.teacher_secret_question.clone();

        let mut app = Self {
            settings,
            base_path,
            modules,
            tabs: vec![
                Tab {
                    id: 0,
                    title: "Home".to_string(),
                    kind: TabKind::Home,
                    closable: false,
                    key: "home".to_string(),
                },
                Tab {
                    id: 1,
                    title: "Chat".to_string(),
                    kind: TabKind::Chat,
                    closable: false,
                    key: "chat".to_string(),
                },
            ],
            active_tab: 0,
            next_tab_id: 2,
            chat_input: String::new(),
            chat_log: Vec::new(),
            memory_jogger,
            bookkeeper,
            bookkeeper_query: String::new(),
            bookkeeper_results: Vec::new(),
            bookkeeper_status: None,
            theme,
            ecg_window: EcgWindowState::new("ECG Window - System hardware activity"),
            presets,
            allow_external_process: false,
            current_pack: pack,
            submissions,
            selected_assignment: initial_selected,
            submission_text: String::new(),
            draft_assignments: Vec::new(),
            draft_input: AssignmentDraft {
                id: "hw-001".to_string(),
                title: "Homework title".to_string(),
                subject: "General".to_string(),
                year_level: "7".to_string(),
                due_at: "".to_string(),
                instructions_md: "Add instructions here.".to_string(),
                allow_games: false,
                allow_ai_premark: true,
                max_score: "100".to_string(),
            },
            selected_students: HashSet::new(),
            submission_attachments: Vec::new(),
            available_models: models,
            teacher_unlocked: false,
            teacher_pin_input: String::new(),
            teacher_pin_new: String::new(),
            teacher_pin_confirm: String::new(),
            teacher_pin_status: None,
            teacher_secret_answer_input: String::new(),
            teacher_secret_question_input: teacher_secret_question,
            revision_sources: Vec::new(),
            revision_progress: HashMap::new(),
            selected_revision: None,
            revision_notes: String::new(),
            revision_confidence: 50,
            revision_status: None,
            revision_chat_input: String::new(),
            revision_chat_log: Vec::new(),
            past_papers: Vec::new(),
            teacher_pack_request: String::new(),
            teacher_pack_markdown: String::new(),
            teacher_pack_status: None,
            teacher_tools_status: None,
            teacher_revision_status: None,
            diagnostics_report: String::new(),
            diagnostics_audience: DiagnosticsAudience::StudentSafe,
            diagnostics_status: None,
            homework_question_index: Vec::new(),
        };
        app.refresh_homework_question_index();
        app.resync_revision();
        Ok(app)
    }

    fn reload_modules(&mut self) {
        self.modules = load_modules(&self.base_path).unwrap_or_default();
        self.pulse_ecg(18.0, "Reloaded module manifests.");
    }

    fn reload_models(&mut self) {
        self.available_models = discover_local_models(&self.base_path);
        self.pulse_ecg(18.0, "Rescanned local model files.");
    }

    fn select_model(&mut self, model: &LocalModelFile) {
        self.settings.model.name = model.name.clone();
        self.settings.model.path = model.path.to_string_lossy().to_string();
        local_model::clear_cached_model();
        if let Err(e) = save_settings(&self.settings, &self.base_path) {
            eprintln!("[models] Failed to save selected model: {e}");
        }
        self.pulse_ecg(34.0, "Switched the active local model.");
    }

    fn current_role(&self) -> &str {
        if self.teacher_unlocked {
            "teacher"
        } else {
            "student"
        }
    }

    fn try_unlock_teacher(&mut self) {
        if self.settings.teacher_pin == self.teacher_pin_input.trim() {
            self.teacher_unlocked = true;
            self.teacher_pin_status = Some("Teacher view unlocked".to_string());
            self.pulse_ecg(26.0, "Teacher view unlocked.");
        } else {
            self.teacher_pin_status = Some("Incorrect PIN".to_string());
        }
        self.teacher_pin_input.clear();
    }

    fn lock_teacher(&mut self) {
        self.teacher_unlocked = false;
        self.close_tabs_by_key_prefix("bookkeeper");
        self.teacher_pin_status = Some("Teacher view locked".to_string());
        self.pulse_ecg(12.0, "Teacher view locked.");
    }

    fn change_teacher_pin(&mut self) {
        if !self.teacher_unlocked {
            self.teacher_pin_status = Some("Unlock first to change PIN".to_string());
            return;
        }
        if self.teacher_pin_new.trim().is_empty() {
            self.teacher_pin_status = Some("PIN cannot be empty".to_string());
            return;
        }
        if self.teacher_pin_new != self.teacher_pin_confirm {
            self.teacher_pin_status = Some("PINs did not match".to_string());
            return;
        }
        self.settings.teacher_pin = self.teacher_pin_new.trim().to_string();
        self.teacher_pin_new.clear();
        self.teacher_pin_confirm.clear();
        match save_settings(&self.settings, &self.base_path) {
            Ok(_) => {
                self.teacher_pin_status = Some("PIN updated".to_string());
                self.pulse_ecg(20.0, "Teacher PIN updated.");
            }
            Err(e) => self.teacher_pin_status = Some(format!("Failed to save PIN: {e}")),
        }
    }

    fn update_secret_question(&mut self) {
        if !self.teacher_unlocked {
            self.teacher_pin_status = Some("Unlock first to change secret question".to_string());
            return;
        }
        if self.teacher_secret_question_input.trim().is_empty()
            || self.teacher_secret_answer_input.trim().is_empty()
        {
            self.teacher_pin_status =
                Some("Secret question and answer cannot be empty".to_string());
            return;
        }
        self.settings.teacher_secret_question =
            self.teacher_secret_question_input.trim().to_string();
        self.settings.teacher_secret_answer = self.teacher_secret_answer_input.trim().to_string();
        match save_settings(&self.settings, &self.base_path) {
            Ok(_) => {
                self.teacher_pin_status = Some("Secret question updated".to_string());
                self.pulse_ecg(20.0, "Teacher recovery question updated.");
            }
            Err(e) => self.teacher_pin_status = Some(format!("Failed to save secret: {e}")),
        }
        self.teacher_secret_answer_input.clear();
    }

    fn open_teacher_dashboard(&mut self) {
        if let Some(module) = self
            .modules
            .iter()
            .find(|m| m.manifest.id == "homework_dashboard")
            .cloned()
        {
            self.open_module_tab(&module);
        }
    }

    fn open_revision_workspace(&mut self) {
        if let Some(module) = self
            .modules
            .iter()
            .find(|m| m.manifest.id == "homework_assignments")
            .cloned()
        {
            self.open_module_tab(&module);
        }
    }

    fn switch_theme(&mut self, name: &str, ctx: &Context) {
        self.theme = load_theme(&self.base_path, Some(name));
        apply_theme(&self.theme, ctx);
        self.settings.ui.last_theme = Some(self.theme.name.clone());
        let _ = save_theme(&self.base_path, &self.theme);
        let _ = save_settings(&self.settings, &self.base_path);
        self.pulse_ecg(10.0, "Updated the current theme.");
    }

    fn unique_assignments_by_id<'a>(pack: &'a HomeworkPack) -> Vec<&'a HomeworkAssignment> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();

        for assignment in &pack.assignments {
            let key = if assignment.id.trim().is_empty() {
                format!("title::{}", assignment.title.trim().to_ascii_lowercase())
            } else {
                assignment.id.trim().to_ascii_lowercase()
            };
            if seen.insert(key) {
                out.push(assignment);
            }
        }

        out
    }

    fn refresh_homework_question_index(&mut self) {
        self.homework_question_index = self
            .current_pack
            .as_ref()
            .map(Self::extract_homework_question_index)
            .unwrap_or_default();

        if let Some(pack) = self.current_pack.as_ref() {
            let visible_assignments = Self::unique_assignments_by_id(pack);
            let selected_is_valid = self.selected_assignment.as_ref().is_some_and(|id| {
                visible_assignments
                    .iter()
                    .any(|assignment| &assignment.id == id)
            });
            if !selected_is_valid {
                self.selected_assignment = visible_assignments.first().map(|a| a.id.clone());
            }
        } else {
            self.selected_assignment = None;
        }
    }

    fn extract_homework_question_index(pack: &HomeworkPack) -> Vec<HomeworkQuestionIntercept> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();

        for assignment in Self::unique_assignments_by_id(pack) {
            let mut sources = vec![assignment.instructions_md.clone()];
            if let Some(printable) = assignment.student_printable_md.as_deref() {
                let cleaned = Self::clean_markdown_fences(printable);
                if !cleaned.is_empty()
                    && Self::normalize_compare_text(&cleaned)
                        != Self::normalize_compare_text(&assignment.instructions_md)
                {
                    sources.push(cleaned);
                }
            }

            for source in sources {
                for (question_number, question_text) in Self::extract_question_candidates(&source) {
                    let normalized_question = Self::normalize_homework_match_text(&question_text);
                    if normalized_question.is_empty() {
                        continue;
                    }

                    let dedupe_key = format!(
                        "{}::{}",
                        assignment.id.trim().to_ascii_lowercase(),
                        normalized_question
                    );
                    if !seen.insert(dedupe_key) {
                        continue;
                    }

                    let tokens = Self::homework_match_tokens(&normalized_question);
                    if tokens.is_empty() {
                        continue;
                    }

                    out.push(HomeworkQuestionIntercept {
                        assignment_id: assignment.id.clone(),
                        question_number,
                        question_text,
                        normalized_question: normalized_question.clone(),
                        keyword_tokens: Self::homework_keyword_tokens(&tokens),
                        number_tokens: Self::homework_number_tokens(&tokens),
                        signature_phrases: Self::homework_signature_phrases(
                            &normalized_question,
                            &tokens,
                        ),
                        tokens,
                    });
                }
            }
        }

        out
    }

    fn resync_homework(&mut self) {
        self.current_pack = find_latest_pack(&self.base_path)
            .ok()
            .flatten()
            .map(|(_p, pack)| pack);
        self.submissions = load_submission_summaries(&self.base_path).unwrap_or_default();
        self.refresh_homework_question_index();
        self.pulse_ecg(28.0, "Rescanned homework packs and submissions.");
    }

    fn resync_revision(&mut self) {
        self.revision_sources = load_revision_sources(
            &self.base_path,
            Some(self.settings.student.student_id.as_str()),
        )
        .unwrap_or_default();
        self.revision_progress = load_revision_progress(&self.base_path)
            .unwrap_or_default()
            .into_iter()
            .map(|progress| (progress.revision_key.clone(), progress))
            .collect();
        let progress_map = self.revision_progress.clone();
        self.revision_sources.sort_by(|a, b| {
            let a_confidence_gap = progress_map
                .get(&a.revision_key)
                .map(|progress| 100 - progress.confidence.clamp(0, 100))
                .unwrap_or(0);
            let b_confidence_gap = progress_map
                .get(&b.revision_key)
                .map(|progress| 100 - progress.confidence.clamp(0, 100))
                .unwrap_or(0);
            let a_review_count = progress_map
                .get(&a.revision_key)
                .map(|progress| progress.review_count)
                .unwrap_or(0);
            let b_review_count = progress_map
                .get(&b.revision_key)
                .map(|progress| progress.review_count)
                .unwrap_or(0);

            b_confidence_gap
                .cmp(&a_confidence_gap)
                .then_with(|| revision_priority(b).cmp(&revision_priority(a)))
                .then_with(|| a_review_count.cmp(&b_review_count))
                .then_with(|| b.submitted_at.cmp(&a.submitted_at))
        });
        self.past_papers = load_past_papers(&self.base_path).unwrap_or_default();

        let revision_is_valid = self.selected_revision.as_ref().is_some_and(|key| {
            self.revision_sources
                .iter()
                .any(|source| &source.revision_key == key)
        });
        if !revision_is_valid {
            self.selected_revision = self
                .revision_sources
                .first()
                .map(|source| source.revision_key.clone());
        }
        self.sync_revision_editor_from_selection();
    }

    fn sync_revision_editor_from_selection(&mut self) {
        if let Some(key) = self.selected_revision.clone() {
            if let Some(progress) = self.revision_progress.get(&key) {
                self.revision_notes = progress.notes.clone();
                self.revision_confidence = progress.confidence.clamp(0, 100);
                return;
            }
        }
        self.revision_notes.clear();
        self.revision_confidence = 50;
    }

    fn open_diagnostics_tab(&mut self) {
        self.refresh_diagnostics_report();
        self.pulse_ecg(18.0, "Opened diagnostics.");
        self.open_or_focus_tab("diagnostics", |_app| Tab {
            id: 0,
            title: "Diagnostics".to_string(),
            kind: TabKind::Diagnostics,
            closable: true,
            key: "diagnostics".to_string(),
        });
    }

    fn refresh_diagnostics_report(&mut self) {
        let audience = if self.teacher_unlocked {
            DiagnosticsAudience::Teacher
        } else {
            DiagnosticsAudience::StudentSafe
        };
        self.diagnostics_audience = audience;
        self.diagnostics_report = self.build_diagnostics_report(audience);
        self.pulse_ecg(16.0, "Refreshed the diagnostic report.");
    }

    fn pulse_ecg(&mut self, intensity: f32, note: &str) {
        self.ecg_window.record_activity(intensity, note);
    }

    fn build_diagnostics_report(&self, audience: DiagnosticsAudience) -> String {
        let yes_no = |b: bool| if b { "yes" } else { "no" };
        let now = Utc::now().to_rfc3339();
        let build = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let local_model = if cfg!(feature = "local-model") {
            "enabled"
        } else {
            "disabled"
        };

        let mut warnings: Vec<String> = Vec::new();
        let mut out = String::new();
        out.push_str("Chatty-EDU Diagnostic / Health Check\n");
        out.push_str(&format!(
            "Version: {} ({build})\n",
            env!("CARGO_PKG_VERSION")
        ));
        out.push_str(&format!("Timestamp (UTC): {now}\n"));
        out.push_str(&format!(
            "Audience: {}\n",
            match audience {
                DiagnosticsAudience::StudentSafe => "student-safe",
                DiagnosticsAudience::Teacher => "teacher",
            }
        ));
        out.push_str(&format!(
            "OS/Arch: {}/{}\n",
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        out.push_str(&format!("Local-model feature: {local_model}\n"));
        out.push_str(&format!("Base path: {}\n", self.base_path.display()));
        if let Ok(exe) = std::env::current_exe() {
            out.push_str(&format!("Executable: {}\n", exe.display()));
        }
        out.push('\n');

        let base = &self.base_path;
        let dirs: Vec<(&str, PathBuf)> = vec![
            ("config", base.join("config")),
            ("config/bookkeeper", bookkeeper_dir(base)),
            ("models", base.join("models")),
            ("homework/assigned", base.join("homework").join("assigned")),
            ("homework/outgoing", base.join("homework").join("outgoing")),
            (
                "homework/completed",
                base.join("homework").join("completed"),
            ),
            ("homework/marking", base.join("homework").join("marking")),
            (
                "homework/printables",
                base.join("homework").join("printables"),
            ),
            ("homework/rubrics", base.join("homework").join("rubrics")),
            ("modules", base.join("modules")),
            ("themes", base.join("themes")),
            ("runtime", base.join("runtime")),
            ("logs", base.join("logs")),
        ];

        out.push_str("Folders:\n");
        for (label, path) in &dirs {
            let exists = path.exists();
            if !exists {
                warnings.push(format!("Missing folder: {label}"));
            }
            out.push_str(&format!(
                "- {label}: {} ({})\n",
                if exists { "ok" } else { "missing" },
                path.display()
            ));
        }
        out.push('\n');

        // Basic writability check (best-effort).
        let write_test_path = base.join("runtime").join("health_check_write_test.tmp");
        match fs::write(&write_test_path, "ok") {
            Ok(_) => {
                let _ = fs::remove_file(&write_test_path);
                out.push_str("Write test: ok (runtime)\n\n");
            }
            Err(err) => {
                warnings.push("Base path not writable (write test failed)".to_string());
                out.push_str(&format!("Write test: FAILED ({err})\n\n"));
            }
        }

        out.push_str("Model:\n");
        out.push_str(&format!("- name: {}\n", self.settings.model.name));
        out.push_str(&format!("- path: {}\n", self.settings.model.path));
        out.push_str(&format!(
            "- max_tokens: {}\n",
            self.settings.model.max_tokens
        ));
        out.push_str(&format!(
            "- bookkeeper_role: {}\n",
            if self.settings.bookkeeper_model_name.trim().is_empty() {
                "Keyword-only background summary mode"
            } else {
                &self.settings.bookkeeper_model_name
            }
        ));
        if audience == DiagnosticsAudience::Teacher
            && !self.settings.bookkeeper_model_path.is_empty()
        {
            out.push_str(&format!(
                "- bookkeeper_path: {}\n",
                self.settings.bookkeeper_model_path
            ));
        }
        let model_path = Path::new(&self.settings.model.path);
        let model_exists = !self.settings.model.path.trim().is_empty() && model_path.exists();
        if !model_exists {
            warnings.push("Model file not found (check File -> Models)".to_string());
        }
        out.push_str(&format!("- exists: {}\n", yes_no(model_exists)));
        if model_exists {
            if let Ok(meta) = fs::metadata(model_path) {
                out.push_str(&format!("- size_bytes: {}\n", meta.len()));
            }
            let mut gguf_ok = false;
            match gguf_magic_ok(model_path) {
                Ok(true) => {
                    gguf_ok = true;
                    out.push_str("- gguf_magic: ok\n");
                }
                Ok(false) => {
                    warnings.push("Model file is not GGUF (missing GGUF magic)".to_string());
                    out.push_str("- gguf_magic: FAILED (not GGUF)\n");
                }
                Err(e) => {
                    warnings.push("Could not read model header".to_string());
                    out.push_str(&format!("- gguf_magic: error ({e})\n"));
                }
            }
            if gguf_ok {
                match gguf_metadata_summary(model_path) {
                    Ok(summary) => {
                        out.push_str(&summary);
                    }
                    Err(err) => {
                        warnings.push("Could not parse GGUF metadata".to_string());
                        out.push_str(&format!("- gguf_metadata: error ({err})\n"));
                    }
                }
            }
        }
        out.push('\n');

        out.push_str("Homework:\n");
        out.push_str(&format!(
            "- current_pack_loaded: {}\n",
            yes_no(self.current_pack.is_some())
        ));
        if let Some(pack) = &self.current_pack {
            out.push_str(&format!("- school_id: {}\n", pack.school_id));
            out.push_str(&format!("- class_id: {}\n", pack.class_id));
            out.push_str(&format!("- assignments: {}\n", pack.assignments.len()));
        } else {
            warnings.push("No homework pack loaded".to_string());
        }
        out.push_str(&format!(
            "- submissions_loaded: {}\n",
            self.submissions.len()
        ));
        out.push_str(&format!("- modules_loaded: {}\n", self.modules.len()));
        out.push('\n');

        if matches!(audience, DiagnosticsAudience::Teacher) {
            out.push_str("Teacher settings:\n");
            out.push_str(&format!(
                "- teacher_unlocked_this_session: {}\n",
                yes_no(self.teacher_unlocked)
            ));
            out.push_str(&format!("- teacher_mode: {}\n", self.settings.teacher_mode));
            out.push_str(&format!(
                "- homework_hints_only: {}\n",
                yes_no(self.settings.homework_hints_only)
            ));
            out.push_str(&format!(
                "- janet_enabled: {}\n",
                yes_no(self.settings.janet.enabled)
            ));
            out.push_str(&format!(
                "- janet_block_swears: {}\n",
                yes_no(self.settings.janet.block_swears)
            ));
            out.push_str(&format!(
                "- janet_block_mature_topics: {}\n",
                yes_no(self.settings.janet.block_mature_topics)
            ));
            out.push_str(&format!(
                "- games_enabled: {}\n",
                yes_no(self.settings.game.enabled)
            ));
            out.push_str(&format!(
                "- games_in_class_allowed: {}\n",
                yes_no(self.settings.game.games_in_class_allowed)
            ));

            let pin_is_default = self.settings.teacher_pin.trim() == "0000";
            if pin_is_default {
                warnings.push("Teacher PIN is still default (0000)".to_string());
            }
            out.push_str(&format!(
                "- teacher_pin_is_default: {}\n",
                yes_no(pin_is_default)
            ));
            out.push_str(&format!(
                "- secret_question_set: {}\n",
                yes_no(!self.settings.teacher_secret_question.trim().is_empty())
            ));
            out.push('\n');
        }

        if !warnings.is_empty() {
            out.push_str("Warnings:\n");
            for w in warnings {
                out.push_str(&format!("- {w}\n"));
            }
            out.push('\n');
        }

        out.push_str("Notes:\n");
        out.push_str("- This report omits student IDs/names and secrets by design.\n");
        out.push_str("- Copy/paste this report when asking for support.\n");

        out
    }

    fn open_or_focus_tab(&mut self, key: &str, builder: impl FnOnce(&mut Self) -> Tab) {
        if let Some(idx) = self.tabs.iter().position(|t| t.key == key) {
            self.active_tab = idx;
            return;
        }
        let mut tab = builder(self);
        tab.id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    fn close_tabs_by_key_prefix(&mut self, prefix: &str) {
        let active_key = self
            .tabs
            .get(self.active_tab)
            .map(|tab| tab.key.clone())
            .unwrap_or_default();
        self.tabs.retain(|tab| !tab.key.starts_with(prefix));
        if self.tabs.is_empty() {
            self.active_tab = 0;
            return;
        }
        if active_key.starts_with(prefix) {
            self.active_tab = 0;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
    }

    fn refresh_bookkeeper_search(&mut self) {
        let results = self
            .bookkeeper
            .as_ref()
            .map(|bookkeeper| bookkeeper.search(&self.bookkeeper_query))
            .unwrap_or_default();
        self.bookkeeper_status = Some(if self.bookkeeper_query.trim().is_empty() {
            format!("Showing {} most recent log entries.", results.len())
        } else {
            format!(
                "Showing {} match(es) for \"{}\".",
                results.len(),
                self.bookkeeper_query.trim()
            )
        });
        self.bookkeeper_results = results;
    }

    fn open_bookkeeper_tab(&mut self) {
        if !self.teacher_unlocked {
            self.teacher_pin_status =
                Some("Unlock teacher view to open bookkeeper logs.".to_string());
            return;
        }
        self.refresh_bookkeeper_search();
        self.open_or_focus_tab("bookkeeper", |_app| Tab {
            id: 0,
            title: "Bookkeeper".to_string(),
            kind: TabKind::Bookkeeper,
            closable: true,
            key: "bookkeeper".to_string(),
        });
    }

    fn open_module_tab(&mut self, module: &LoadedModule) {
        if module.manifest.id == "homework_dashboard" && !self.teacher_unlocked {
            self.teacher_pin_status =
                Some("Unlock teacher view to open the homework dashboard.".to_string());
            return;
        }
        self.pulse_ecg(12.0, &format!("Opened {}.", module.manifest.title));
        let key = format!("module:{}", module.manifest.id);
        let m = module.clone();
        let tab_key = key.clone();
        self.open_or_focus_tab(&key, |_app| Tab {
            id: 0,
            title: m.manifest.title.clone(),
            kind: TabKind::Module {
                module: m,
                cached_text: None,
            },
            closable: true,
            key: tab_key,
        });
    }

    fn close_tab(&mut self, idx: usize) {
        if idx < self.tabs.len() && self.tabs[idx].closable {
            self.tabs.remove(idx);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len().saturating_sub(1);
            }
        }
    }

    fn render_menu_bar(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Reload modules").clicked() {
                    self.reload_modules();
                    ui.close_menu();
                }
                ui.menu_button("Files", |ui| {
                    ui.label(format!("Base path: {}", self.base_path.display()));
                    if ui.button("Open Diagnostic / Health Check").clicked() {
                        self.open_diagnostics_tab();
                        ui.close_menu();
                    }
                    if ui.button("Copy Diagnostic report").clicked() {
                        let audience = if self.teacher_unlocked {
                            DiagnosticsAudience::Teacher
                        } else {
                            DiagnosticsAudience::StudentSafe
                        };
                        let report = self.build_diagnostics_report(audience);
                        ctx.copy_text(report.clone());
                        self.diagnostics_audience = audience;
                        self.diagnostics_report = report;
                        self.diagnostics_status =
                            Some("Copied diagnostic report to clipboard.".to_string());
                        ui.close_menu();
                    }
                    if let Some(msg) = &self.diagnostics_status {
                        ui.separator();
                        ui.label(msg);
                    }
                });
                ui.menu_button("Models", |ui| {
                    let models = self.available_models.clone();
                    let current_path = self.settings.model.path.clone();
                    let main_role_label = if self.settings.model.path.trim().is_empty() {
                        "No model selected".to_string()
                    } else {
                        self.settings.model.name.clone()
                    };
                    if models.is_empty() {
                        ui.label("No GGUF models found yet.");
                        ui.label("Drop a GGUF into data/models/ to get started.");
                    } else {
                        ui.label(format!("Main AI role: {main_role_label}"));
                        if self.teacher_unlocked {
                            ui.label(format!(
                                "Bookkeeper role: {}",
                                if self.settings.bookkeeper_model_name.trim().is_empty() {
                                    "Keyword-only background summary mode"
                                } else {
                                    &self.settings.bookkeeper_model_name
                                }
                            ));
                        }
                        ui.separator();
                    }
                    for model in models {
                        let selected = Path::new(&current_path) == model.path;
                        let file_label = model
                            .path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| model.path.to_string_lossy().to_string());
                        let label = format!("{} ({})", model.name, file_label);
                        if ui.selectable_label(selected, label).clicked() {
                            self.select_model(&model);
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button("Refresh models list").clicked() {
                        self.reload_models();
                        ui.close_menu();
                    }
                    if self.teacher_unlocked {
                        ui.separator();
                        if ui.button("Open bookkeeper logs").clicked() {
                            self.open_bookkeeper_tab();
                            ui.close_menu();
                        }
                    }
                    ui.label(format!(
                        "Folder: {}",
                        self.base_path.join("models").display()
                    ));
                });
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.menu_button("View", |ui| {
                let preset_names: Vec<String> =
                    self.presets.iter().map(|p| p.name.clone()).collect();
                for name in preset_names {
                    let selected = self.theme.name == name;
                    if ui.selectable_label(selected, name.clone()).clicked() {
                        self.switch_theme(&name, ctx);
                        ui.close_menu();
                    }
                }
            });

            ui.menu_button("Modules", |ui| {
                let current_role = self.current_role().to_owned();
                if self.modules.is_empty() {
                    ui.label("No modules found.");
                }
                let modules = self.modules.clone();
                for module in modules {
                    if module.manifest.id == "homework_dashboard" && !self.teacher_unlocked {
                        continue;
                    }
                    if !role_allowed(&module.manifest, current_role.as_str()) {
                        continue;
                    }
                    if ui.button(module.manifest.title.clone()).clicked() {
                        self.open_module_tab(&module);
                        ui.close_menu();
                    }
                }
            });

            ui.menu_button("Tools", |ui| {
                ui.add_enabled(false, egui::Label::new("Coming soon"));
            });

            ui.menu_button("Teacher", |ui| {
                ui.label(format!(
                    "Status: {}",
                    if self.teacher_unlocked {
                        "Unlocked"
                    } else {
                        "Locked"
                    }
                ));
                ui.label(format!("Role: {}", self.current_role()));
                ui.separator();
                if !self.teacher_unlocked {
                    ui.label("Enter PIN to unlock teacher view");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.teacher_pin_input)
                            .password(true)
                            .hint_text("0000"),
                    );
                    if ui.button("Unlock").clicked() {
                        self.try_unlock_teacher();
                        ui.close_menu();
                    }
                    if !self.settings.teacher_secret_question.is_empty() {
                        ui.separator();
                        ui.label("Forgot PIN? Answer secret question:");
                        ui.label(format!("Q: {}", self.settings.teacher_secret_question));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.teacher_secret_answer_input)
                                .password(true)
                                .hint_text("Answer"),
                        );
                        if ui.button("Unlock with answer").clicked() {
                            if self.settings.teacher_secret_answer
                                == self.teacher_secret_answer_input.trim()
                            {
                                self.teacher_unlocked = true;
                                self.teacher_pin_status =
                                    Some("Unlocked via secret question".to_string());
                            } else {
                                self.teacher_pin_status =
                                    Some("Incorrect secret answer".to_string());
                            }
                            self.teacher_secret_answer_input.clear();
                            ui.close_menu();
                        }
                    }
                } else {
                    if ui.button("Open teacher dashboard").clicked() {
                        self.open_teacher_dashboard();
                        ui.close_menu();
                    }
                    if ui.button("Rescan packs + submissions").clicked() {
                        self.resync_homework();
                        self.resync_revision();
                        self.teacher_pin_status =
                            Some("Rescanned packs and submissions.".to_string());
                    }
                    ui.separator();
                    ui.label("Class mode");
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.settings.teacher_mode != "class",
                                egui::Button::new("Set CLASS"),
                            )
                            .clicked()
                        {
                            self.settings.teacher_mode = "class".to_string();
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status =
                                Some("Teacher mode set to CLASS.".to_string());
                        }
                        if ui
                            .add_enabled(
                                self.settings.teacher_mode != "free_time",
                                egui::Button::new("Set FREE TIME"),
                            )
                            .clicked()
                        {
                            self.settings.teacher_mode = "free_time".to_string();
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status =
                                Some("Teacher mode set to FREE TIME.".to_string());
                        }
                        ui.label(format!("Current: {}", self.settings.teacher_mode));
                    });
                    ui.separator();
                    ui.label("Games");
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!self.settings.game.enabled, egui::Button::new("Games ON"))
                            .clicked()
                        {
                            self.settings.game.enabled = true;
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status = Some("Games enabled.".to_string());
                        }
                        if ui
                            .add_enabled(self.settings.game.enabled, egui::Button::new("Games OFF"))
                            .clicked()
                        {
                            self.settings.game.enabled = false;
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status = Some("Games disabled.".to_string());
                        }
                        ui.label(format!(
                            "Allowed in class: {}",
                            self.settings.game.games_in_class_allowed
                        ));
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.settings.game.games_in_class_allowed,
                                egui::Button::new("Allow in class"),
                            )
                            .clicked()
                        {
                            self.settings.game.games_in_class_allowed = true;
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status =
                                Some("Games allowed in class.".to_string());
                        }
                        if ui
                            .add_enabled(
                                self.settings.game.games_in_class_allowed,
                                egui::Button::new("Forbid in class"),
                            )
                            .clicked()
                        {
                            self.settings.game.games_in_class_allowed = false;
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status =
                                Some("Games forbidden in class.".to_string());
                        }
                    });
                    ui.separator();
                    if ui.button("Export pack template").clicked() {
                        match export_pack_template(
                            &self.base_path,
                            "school",
                            &self.settings.student.class_id,
                        ) {
                            Ok(path) => {
                                self.teacher_pin_status =
                                    Some(format!("Template written to {}", path.display()));
                            }
                            Err(e) => {
                                self.teacher_pin_status =
                                    Some(format!("Failed to export template: {e}"));
                            }
                        }
                    }
                    if ui.button("Import pack file...").clicked() {
                        if let Some(file) = FileDialog::new()
                            .add_filter("homework pack", &["json", "md"])
                            .pick_file()
                        {
                            let ext = file
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_ascii_lowercase();

                            if ext == "md" {
                                let outgoing_dir = homework_markdown::homework_outgoing_dir(&self.base_path);
                                let _ = fs::create_dir_all(&outgoing_dir);
                                let file_name = file
                                    .file_name()
                                    .unwrap_or_else(|| std::ffi::OsStr::new("homework_pack_import.md"));
                                let mut dest_md = outgoing_dir.join(file_name);
                                let mut n = 1usize;
                                while dest_md.exists() {
                                    dest_md = outgoing_dir.join(format!(
                                        "{}_{n}.md",
                                        dest_md
                                            .file_stem()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("homework_pack_import")
                                    ));
                                    n += 1;
                                }
                                if let Err(e) = fs::copy(&file, &dest_md) {
                                    self.teacher_pin_status = Some(format!("Import failed: {e}"));
                                } else {
                                    let defaults = PackMdDefaults {
                                        version: "1.0".to_string(),
                                        school_id: self
                                            .current_pack
                                            .as_ref()
                                            .map(|p| p.school_id.clone())
                                            .unwrap_or_else(|| "school".to_string()),
                                        class_id: if self.settings.student.class_id.trim().is_empty() {
                                            self.current_pack
                                                .as_ref()
                                                .map(|p| p.class_id.clone())
                                                .unwrap_or_else(|| "class".to_string())
                                        } else {
                                            self.settings.student.class_id.trim().to_string()
                                        },
                                    };
                                    match homework_markdown::transcribe_outgoing_packs(&self.base_path, &defaults) {
                                        Ok(report) => {
                                            self.resync_homework();
                                            if let Some(pack) = self.current_pack.clone() {
                                                apply_pack_policy(&mut self.settings, &pack);
                                                let _ = save_settings(&self.settings, &self.base_path);
                                            }
                                            self.teacher_pin_status = Some(format!(
                                                "Imported .md to outgoing and transcribed: processed {}, wrote {}, skipped {}, failed {}",
                                                report.processed, report.written, report.skipped, report.failed
                                            ));
                                        }
                                        Err(e) => {
                                            self.teacher_pin_status =
                                                Some(format!("Transcribe failed: {e}"));
                                        }
                                    }
                                }
                            } else {
                                let dest_dir = self.base_path.join("homework").join("assigned");
                                let _ = fs::create_dir_all(&dest_dir);
                                let dest = dest_dir.join(
                                    file.file_name()
                                        .unwrap_or_else(|| std::ffi::OsStr::new("homework_pack_import.json")),
                                );
                                match fs::copy(&file, &dest) {
                                    Ok(_) => match load_pack_from_file(&dest) {
                                        Ok(pack) => {
                                            apply_pack_policy(&mut self.settings, &pack);
                                            let _ = save_settings(&self.settings, &self.base_path);
                                            self.current_pack = Some(pack);
                                            self.resync_homework();
                                            self.teacher_pin_status =
                                                Some(format!("Imported {}", dest.display()));
                                        }
                                        Err(e) => {
                                            self.teacher_pin_status =
                                                Some(format!("Copied but failed to parse pack: {e}"));
                                        }
                                    },
                                    Err(e) => {
                                        self.teacher_pin_status =
                                            Some(format!("Import failed: {e}"));
                                    }
                                }
                            }
                        }
                    }
                    if ui.button("Show completed summary").clicked() {
                        let rows = self.submission_rows();
                        if rows.is_empty() {
                            self.teacher_pin_status =
                                Some("No completed submissions found.".to_string());
                        } else {
                            egui::Window::new("Completed submissions")
                                .id(egui::Id::new("teacher_completed_submissions_window"))
                                .collapsible(true)
                                .resizable(true)
                                .show(ui.ctx(), |ui| {
                                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                        for row in &rows {
                                            ui.push_id(
                                                (
                                                    "teacher_completed_summary_row",
                                                    &row.assignment_id,
                                                    &row.student_id,
                                                    &row.submitted_at,
                                                ),
                                                |ui| {
                                                    let label = format!(
                                                        "{} ({}) - {} ({}) - subj: {} - score: {} - {} - submitted: {}",
                                                        row.assignment_title,
                                                        row.assignment_id,
                                                        row.student_name,
                                                        row.student_id,
                                                        row.subject,
                                                        row.score,
                                                        row.feedback,
                                                        row.submitted_at
                                                    );
                                                    ui.label(&label);
                                                    if let Some(ai_fb) = self
                                                        .submissions
                                                        .iter()
                                                        .find(|s| {
                                                            s.assignment_id == row.assignment_id
                                                                && s.student_id
                                                                    == row.student_id
                                                        })
                                                        .and_then(|s| s.ai_feedback.clone())
                                                    {
                                                        ui.label(format!(
                                                            "AI feedback: {}",
                                                            ai_fb
                                                        ));
                                                    }
                                                },
                                            );
                                        }
                                    });
                                });
                            self.teacher_pin_status =
                                Some(format!("Completed submissions: {}", rows.len()));
                        }
                    }
                    if ui.button("Lock teacher view").clicked() {
                        self.lock_teacher();
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label("Change teacher PIN");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.teacher_pin_new)
                            .password(true)
                            .hint_text("New PIN"),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.teacher_pin_confirm)
                            .password(true)
                            .hint_text("Confirm PIN"),
                    );
                    if ui.button("Update PIN").clicked() {
                        self.change_teacher_pin();
                    }
                    ui.separator();
                    ui.label("Secret question (for PIN recovery)");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.teacher_secret_question_input)
                            .hint_text("Secret question"),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.teacher_secret_answer_input)
                            .password(true)
                            .hint_text("Answer"),
                    );
                    if ui.button("Save secret").clicked() {
                        self.update_secret_question();
                    }
                }
                if let Some(msg) = &self.teacher_pin_status {
                    ui.colored_label(self.warning_color(), msg);
                }
            });

            ui.menu_button("Settings", |ui| {
                if ui.button("Open settings tab").clicked() {
                    self.open_or_focus_tab("settings", |_app| Tab {
                        id: 0,
                        title: "Settings".to_string(),
                        kind: TabKind::Settings,
                        closable: true,
                        key: "settings".to_string(),
                    });
                    ui.close_menu();
                }
                if ui.button("Open diagnostics tab").clicked() {
                    self.open_diagnostics_tab();
                    ui.close_menu();
                }
            });

            ui.menu_button("Help", |ui| {
                ui.label(format!("Chatty-EDU v{} (egui)", env!("CARGO_PKG_VERSION")));
                ui.label(format!("Base path: {}", self.base_path.display()));
            });
        });
    }

    fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        let widget_width = 176.0;
        let gap = 8.0;
        ui.horizontal(|ui| {
            let tab_width = (ui.available_width() - widget_width - gap).max(140.0);
            ui.scope(|ui| {
                ui.set_min_width(tab_width);
                ui.set_max_width(tab_width);
                ui.horizontal_wrapped(|ui| {
                    let mut to_close: Option<usize> = None;
                    for (idx, tab) in self.tabs.iter().enumerate() {
                        let active = idx == self.active_tab;
                        ui.horizontal(|ui| {
                            let response = if matches!(&tab.kind, TabKind::Bookkeeper) {
                                ui.selectable_label(active, tab.title.clone())
                                    .on_hover_text(
                                    "Full session logs. Search past activity and diagnose issues.",
                                )
                            } else {
                                ui.selectable_label(active, tab.title.clone())
                            };
                            if response.clicked() {
                                self.active_tab = idx;
                            }
                            if tab.closable && ui.button("x").clicked() {
                                to_close = Some(idx);
                            }
                        });
                    }

                    if let Some(idx) = to_close {
                        self.close_tab(idx);
                    }
                });
            });
            ui.add_space(gap);
            self.render_ecg_window(ui);
        });
    }

    fn render_ecg_window(&self, ui: &mut egui::Ui) {
        let payload = self.ecg_window.payload();
        let desired_size = egui::vec2(176.0, 46.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

        let surface = color_from_hex(&self.theme.surface);
        let border = color_from_hex(&self.theme.border);
        let muted = color_from_hex(&self.theme.muted_text);
        let accent = if payload.current_percent >= 35.0 {
            color_from_hex(&self.theme.accent)
        } else {
            muted.gamma_multiply(0.9)
        };

        ui.painter().rect(
            rect,
            egui::Rounding::same(6.0),
            surface,
            egui::Stroke::new(1.0, border),
        );

        let inner = rect.shrink2(egui::vec2(8.0, 6.0));
        let small_font = egui::TextStyle::Small.resolve(ui.style());
        let body_font = egui::TextStyle::Body.resolve(ui.style());

        ui.painter().text(
            inner.left_top(),
            egui::Align2::LEFT_TOP,
            "ECG",
            small_font,
            muted,
        );
        ui.painter().text(
            inner.right_top(),
            egui::Align2::RIGHT_TOP,
            format!("{:.0}%", payload.current_percent),
            body_font,
            accent,
        );

        let chart_rect = egui::Rect::from_min_max(
            egui::pos2(inner.min.x, inner.min.y + 16.0),
            egui::pos2(inner.max.x, inner.max.y),
        );
        ui.painter().line_segment(
            [
                egui::pos2(chart_rect.left(), chart_rect.bottom()),
                egui::pos2(chart_rect.right(), chart_rect.bottom()),
            ],
            egui::Stroke::new(1.0, border.gamma_multiply(0.6)),
        );

        let points = self
            .ecg_window
            .points(chart_rect.width(), chart_rect.height())
            .into_iter()
            .map(|point| egui::pos2(chart_rect.left() + point.x, chart_rect.top() + point.y))
            .collect::<Vec<_>>();

        if points.len() >= 2 {
            ui.painter()
                .add(egui::Shape::line(points, egui::Stroke::new(1.6, accent)));
        } else if let Some(point) = points.first() {
            ui.painter().circle_filled(*point, 1.8, accent);
        }

        let state = if !payload.supported {
            "unsupported"
        } else if payload.available {
            "live"
        } else {
            "unavailable"
        };
        let _ = response.on_hover_text(format!(
            "{}\n{}\nCurrent: {:.0}%\nState: {}",
            payload.label, payload.note, payload.current_percent, state
        ));
    }

    fn render_home(&mut self, ui: &mut egui::Ui) {
        ui.heading("Home");
        ui.label(format!("Base path: {}", self.base_path.display()));
        if self.available_models.is_empty() || self.settings.model.path.trim().is_empty() {
            ui.label("AI setup: Drop a GGUF into data/models/ to get started.");
        } else {
            let current_model = Path::new(&self.settings.model.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&self.settings.model.path);
            ui.label(format!(
                "Active model: {} ({current_model})",
                self.settings.model.name
            ));
        }
        ui.label(format!("Teacher mode: {}", self.settings.teacher_mode));
        ui.label(format!("Available modules: {}", self.modules.len()));
        ui.separator();
        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                ui.set_min_height(ui.available_height());
                ui.label(RichText::new("Student profile").strong());
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.settings.student.student_name);
                    ui.label("ID");
                    ui.text_edit_singleline(&mut self.settings.student.student_id);
                    ui.label("Class");
                    ui.text_edit_singleline(&mut self.settings.student.class_id);
                    if ui.button("Save profile").clicked() {
                        let _ = save_settings(&self.settings, &self.base_path);
                        self.resync_revision();
                    }
                });
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Rescan packs + submissions").clicked() {
                        self.resync_homework();
                        self.resync_revision();
                    }
                    if ui
                        .add_enabled(
                            !self.revision_sources.is_empty() || !self.past_papers.is_empty(),
                            egui::Button::new("Open Revision"),
                        )
                        .clicked()
                    {
                        self.open_revision_workspace();
                    }
                    if ui
                        .add_enabled(
                            self.teacher_unlocked,
                            egui::Button::new("Export pack template"),
                        )
                        .clicked()
                    {
                        match export_pack_template(
                            &self.base_path,
                            "school",
                            &self.settings.student.class_id,
                        ) {
                            Ok(path) => {
                                let _ = ui.label(format!("Template at {}", path.display()));
                            }
                            Err(e) => {
                                let _ = ui.label(format!("Failed: {e}"));
                            }
                        };
                    }
                    if ui
                        .add_enabled(
                            self.teacher_unlocked,
                            egui::Button::new("Import pack file..."),
                        )
                        .clicked()
                    {
                        if let Some(file) = FileDialog::new()
                            .add_filter("homework pack", &["json", "md"])
                            .pick_file()
                        {
                            let ext = file
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_ascii_lowercase();

                            if ext == "md" {
                                let outgoing_dir = homework_markdown::homework_outgoing_dir(&self.base_path);
                                let _ = fs::create_dir_all(&outgoing_dir);
                                let file_name = file
                                    .file_name()
                                    .unwrap_or_else(|| std::ffi::OsStr::new("homework_pack_import.md"));
                                let mut dest_md = outgoing_dir.join(file_name);
                                let mut n = 1usize;
                                while dest_md.exists() {
                                    dest_md = outgoing_dir.join(format!(
                                        "{}_{n}.md",
                                        dest_md
                                            .file_stem()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("homework_pack_import")
                                    ));
                                    n += 1;
                                }
                                if let Err(e) = fs::copy(&file, &dest_md) {
                                    let _ = ui.label(format!("Import failed: {e}"));
                                } else {
                                    let defaults = PackMdDefaults {
                                        version: "1.0".to_string(),
                                        school_id: self
                                            .current_pack
                                            .as_ref()
                                            .map(|p| p.school_id.clone())
                                            .unwrap_or_else(|| "school".to_string()),
                                        class_id: if self.settings.student.class_id.trim().is_empty() {
                                            self.current_pack
                                                .as_ref()
                                                .map(|p| p.class_id.clone())
                                                .unwrap_or_else(|| "class".to_string())
                                        } else {
                                            self.settings.student.class_id.trim().to_string()
                                        },
                                    };
                                    match homework_markdown::transcribe_outgoing_packs(&self.base_path, &defaults) {
                                        Ok(report) => {
                                            self.resync_homework();
                                            if let Some(pack) = self.current_pack.clone() {
                                                apply_pack_policy(&mut self.settings, &pack);
                                                let _ = save_settings(&self.settings, &self.base_path);
                                            }
                                            let _ = ui.label(format!(
                                                "Imported .md to outgoing and transcribed: wrote {} (skipped {}, failed {})",
                                                report.written, report.skipped, report.failed
                                            ));
                                        }
                                        Err(e) => {
                                            let _ = ui.label(format!("Transcribe failed: {e}"));
                                        }
                                    }
                                }
                            } else {
                                let dest_dir = self.base_path.join("homework").join("assigned");
                                let _ = fs::create_dir_all(&dest_dir);
                                let dest = dest_dir.join(
                                    file.file_name()
                                        .unwrap_or_else(|| std::ffi::OsStr::new("homework_pack_import.json")),
                                );
                                if let Err(e) = fs::copy(&file, &dest) {
                                    let _ = ui.label(format!("Import failed: {e}"));
                                } else if let Ok(pack) = load_pack_from_file(&dest) {
                                    apply_pack_policy(&mut self.settings, &pack);
                                    let _ = save_settings(&self.settings, &self.base_path);
                                    self.current_pack = Some(pack);
                                    self.resync_homework();
                                    let _ = ui.label(format!("Imported to {}", dest.display()));
                                }
                            }
                        }
                    }
                });

        if self.teacher_unlocked {
            ui.separator();
            ui.heading("Pack builder (teacher)");
            ui.label("Build a multi-assignment pack to share via the portal.");
            ui.horizontal(|ui| {
                ui.label("Assign ID");
                ui.text_edit_singleline(&mut self.draft_input.id);
                ui.label("Title");
                ui.text_edit_singleline(&mut self.draft_input.title);
            });
            ui.horizontal(|ui| {
                ui.label("Subject");
                ui.text_edit_singleline(&mut self.draft_input.subject);
                ui.label("Year / Grade");
                ui.text_edit_singleline(&mut self.draft_input.year_level);
                ui.label("Due at");
                ui.text_edit_singleline(&mut self.draft_input.due_at);
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.draft_input.allow_games, "Allow games");
                ui.checkbox(&mut self.draft_input.allow_ai_premark, "Allow AI premark");
                ui.label("Max score");
                ui.text_edit_singleline(&mut self.draft_input.max_score);
            });
            ui.label("Instructions");
            ui.text_edit_multiline(&mut self.draft_input.instructions_md);

            ui.horizontal(|ui| {
                if ui.button("Add assignment to pack").clicked() {
                    if !self.draft_input.id.trim().is_empty() {
                        let max_score = if self.draft_input.max_score.trim().is_empty() {
                            None
                        } else {
                            self.draft_input.max_score.trim().parse().ok()
                        };
                        let assignment = HomeworkAssignment {
                            id: self.draft_input.id.trim().to_string(),
                            title: self.draft_input.title.trim().to_string(),
                            subject: self.draft_input.subject.trim().to_string(),
                            year_level: self.draft_input.year_level.trim().to_string(),
                            due_at: if self.draft_input.due_at.trim().is_empty() {
                                None
                            } else {
                                Some(self.draft_input.due_at.trim().to_string())
                            },
                            instructions_md: self.draft_input.instructions_md.clone(),
                            student_printable_md: None,
                            teacher_rubric_md: None,
                            attachments: vec![],
                            allow_games: self.draft_input.allow_games,
                            allow_ai_premark: self.draft_input.allow_ai_premark,
                            max_score,
                        };
                        self.draft_assignments.push(assignment);
                        self.draft_input.id =
                            format!("hw-{:03}", self.draft_assignments.len() + 1);
                    }
                }

                if ui.button("Clear draft list").clicked() {
                    self.draft_assignments.clear();
                }

                if ui
                    .add_enabled(
                        !self.draft_assignments.is_empty(),
                        egui::Button::new("Export pack"),
                    )
                    .clicked()
                {
                    let school_id = "school";
                    let class_id = &self.settings.student.class_id;
                    match create_pack_multi(
                        &self.base_path,
                        school_id,
                        class_id,
                        self.draft_assignments.clone(),
                    ) {
                        Ok(path) => {
                            let _ = ui.label(format!("Pack saved to {}", path.display()));
                            self.resync_homework();
                            self.draft_assignments.clear();
                        }
                        Err(e) => {
                            let _ = ui.label(format!("Failed: {e}"));
                        }
                    }
                }
            });

            if !self.draft_assignments.is_empty() {
                ui.label(format!(
                    "Assignments in pack: {}",
                    self.draft_assignments.len()
                ));
            }
        } else {
            ui.separator();
            ui.colored_label(
                self.warning_color(),
                "Teacher tools are locked. Unlock via the Teacher menu to manage packs.",
            );
        }

        if let Some(pack) = self.current_pack.clone() {
            let visible_assignments = Self::unique_assignments_by_id(&pack);
            ui.separator();
            ui.label(format!(
                "Latest homework pack: {} (class {}) assignments: {}",
                pack.school_id,
                pack.class_id,
                visible_assignments.len()
            ));
            ui.horizontal(|ui| {
                ui.label("Select assignment:");
                let current = self
                    .selected_assignment_ref()
                    .map(|a| format!("{} - {}", a.id, a.title))
                    .unwrap_or_else(|| "Choose...".to_string());
                egui::ComboBox::from_id_source("home_assignment_select")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for a in &visible_assignments {
                            let label = format!("{} - {}", a.id, a.title);
                            if ui
                                .selectable_label(
                                    self.selected_assignment.as_ref() == Some(&a.id),
                                    label,
                                )
                                .clicked()
                            {
                                self.selected_assignment = Some(a.id.clone());
                            }
                        }
                    });
            });
            if let Some(assignment) = self.selected_assignment_ref() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} - {}", assignment.id, assignment.title)).strong(),
                    );
                    ui.label(format!(
                        "Subject: {} | Due: {}",
                        assignment.subject,
                        assignment.due_at.as_deref().unwrap_or("-"),
                    ));
                    if !assignment.allow_games {
                        ui.colored_label(self.warning_color(), "Games off");
                    }
                });
                self.render_assignment_materials(ui, assignment);
                ui.separator();
            }

            ui.horizontal(|ui| {
                ui.label("Finished this work and want practice from past submissions?");
                if ui
                    .add_enabled(
                        !self.revision_sources.is_empty() || !self.past_papers.is_empty(),
                        egui::Button::new("Open Revision"),
                    )
                    .clicked()
                {
                    self.open_revision_workspace();
                }
            });
            ui.add_space(6.0);
            ui.heading("Submit work");
            self.render_submission_area(ui);
        } else {
            ui.label(
                "No homework pack found. Drop a homework_pack*.json into homework/assigned/ and click Rescan.",
            );
        }

        if !self.submissions.is_empty() {
            ui.separator();
            ui.heading("Submissions found locally");
            ScrollArea::vertical()
                .id_source("home_tab_submissions_scroll")
                .max_height(200.0)
                .show(ui, |ui| {
                for row in self.submission_rows() {
                    ui.push_id(
                        (
                            "home_submission_row",
                            &row.assignment_id,
                            &row.student_id,
                            &row.submitted_at,
                        ),
                        |ui| {
                            let label = format!(
                                "{} ({}) - {} ({}) | subj: {} | score: {} | {}",
                                row.assignment_title,
                                row.assignment_id,
                                row.student_name,
                                row.student_id,
                                row.subject,
                                row.score,
                                row.feedback
                            );
                            ui.label(label).on_hover_text(format!(
                                "Assignment ID: {} | Student ID: {} | Submitted: {}",
                                row.assignment_id, row.student_id, row.submitted_at
                            ));
                        },
                    );
                }
            });
        }

                ui.add_space(12.0);
                ui.separator();
                ui.heading("Chat");
                ui.label("Use the chat bar at the bottom to message Chatty from Home.");
                self.render_chat_context_banner(ui);
                self.render_home_chat_preview(ui);
            });
    }

    fn render_chat(&mut self, ui: &mut egui::Ui) {
        ui.heading("Chat");
        ui.add_space(6.0);
        let panel_height = ui.available_height();
        let sidebar_width = 320.0;
        let gap = 10.0;

        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(sidebar_width, panel_height),
                Layout::top_down(Align::Min),
                |ui| self.render_chatty_thoughts_panel(ui),
            );
            ui.add_space(gap);

            let center_width = (ui.available_width() - sidebar_width - gap).max(300.0);
            ui.allocate_ui_with_layout(
                egui::vec2(center_width, panel_height),
                Layout::top_down(Align::Min),
                |ui| {
                    self.render_chat_context_banner(ui);
                    self.render_chat_log(ui, ui.available_height());
                },
            );
            ui.add_space(gap);

            ui.allocate_ui_with_layout(
                egui::vec2(sidebar_width, panel_height),
                Layout::top_down(Align::Min),
                |ui| self.render_memory_jogger_panel(ui),
            );
        });
    }

    fn render_chatty_thoughts_panel(&self, ui: &mut egui::Ui) {
        let thoughts = self.chatty_thoughts_display_items();
        self.render_memory_sidebar(
            ui,
            "Chatty's thoughts",
            "This is what Chatty can currently remember in this session. It clears when the app closes.",
            &thoughts,
            "Nothing has built up in this session yet.",
            Some(180),
        );
    }

    fn render_memory_jogger_panel(&self, ui: &mut egui::Ui) {
        let items = self.memory_jogger_items();
        self.render_memory_sidebar(
            ui,
            "Memory jogger",
            "Helps Chatty remember what you've been working on recently across sessions.",
            &items,
            "No recent memory jogger notes yet. It updates when the app closes.",
            Some(180),
        );
    }

    fn render_memory_sidebar(
        &self,
        ui: &mut egui::Ui,
        title: &str,
        tooltip: &str,
        items: &[String],
        empty_message: &str,
        display_clip_chars: Option<usize>,
    ) {
        ui.push_id(("memory_sidebar", title), |ui| {
            egui::Frame::none()
                .fill(color_from_hex(&self.theme.surface))
                .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
                .rounding(egui::Rounding::same(6.0))
                .inner_margin(egui::vec2(10.0, 8.0))
                .show(ui, |ui| {
                    let heading = ui.label(RichText::new(title).strong());
                    let _ = heading.on_hover_text(tooltip);
                    ui.add_space(6.0);

                    ScrollArea::vertical()
                        .id_source(("memory_sidebar_scroll", title))
                        .auto_shrink([false; 2])
                        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                        .show(ui, |ui| {
                            if items.is_empty() {
                                ui.label(
                                    RichText::new(empty_message)
                                        .small()
                                        .color(color_from_hex(&self.theme.muted_text)),
                                );
                                return;
                            }

                            for item in items {
                                let full_text = Self::display_safe_text(item);
                                let display_text = if let Some(max_chars) = display_clip_chars {
                                    Self::clip_chars(&full_text, max_chars)
                                } else {
                                    full_text.clone()
                                };
                                let response = ui.add_sized(
                                    [ui.available_width(), 0.0],
                                    egui::Label::new(
                                        RichText::new(format!("- {display_text}"))
                                            .small()
                                            .color(color_from_hex(&self.theme.text)),
                                    )
                                    .wrap(true),
                                );
                                if display_clip_chars.is_some() {
                                    let _ = response.on_hover_text(full_text);
                                }
                                ui.add_space(6.0);
                            }
                        });
                });
        });
    }

    fn chatty_thoughts_display_items(&self) -> Vec<String> {
        self.chatty_thoughts_items(None)
    }

    fn chatty_thoughts_prompt_block(&self) -> Option<String> {
        let mut lines = Vec::new();

        if let Some(assignment) = self.selected_assignment_ref() {
            lines.push(format!(
                "Current homework: {} ({}) | {} | Year {}",
                assignment.title, assignment.id, assignment.subject, assignment.year_level
            ));
        }

        let pairs = self.recent_chat_exchange_pair_entries(Some(320), 6);
        if !pairs.is_empty() {
            lines.push("Recent message pairs still in context:".to_string());
            for (index, (user_msg, chatty_msg)) in pairs.into_iter().enumerate() {
                let number = index + 1;
                match (user_msg, chatty_msg) {
                    (Some(user_msg), Some(chatty_msg)) => {
                        lines.push(format!("{number}. You: {user_msg}"));
                        lines.push(format!("   Chatty: {chatty_msg}"));
                    }
                    (Some(user_msg), None) => {
                        lines.push(format!("{number}. You: {user_msg}"));
                    }
                    (None, Some(chatty_msg)) => {
                        lines.push(format!("{number}. Chatty: {chatty_msg}"));
                    }
                    (None, None) => {}
                }
            }
        }

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    fn chatty_thoughts_items(&self, clip_chars: Option<usize>) -> Vec<String> {
        let mut thoughts = Vec::new();

        if let Some(assignment) = self.selected_assignment_ref() {
            thoughts.push(format!(
                "Current homework: {} ({}) | {} | Year {}",
                assignment.title, assignment.id, assignment.subject, assignment.year_level
            ));
        }

        for pair in self.recent_chat_exchange_pairs(clip_chars, 4) {
            thoughts.push(pair);
        }

        thoughts.truncate(5);
        thoughts
    }

    fn recent_chat_exchange_pair_entries(
        &self,
        clip_chars: Option<usize>,
        max_pairs: usize,
    ) -> Vec<(Option<String>, Option<String>)> {
        let recent_messages = self
            .chat_log
            .iter()
            .filter(|(_, message)| {
                let trimmed = message.trim();
                !trimmed.is_empty() && trimmed != "..."
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut pairs = Vec::new();
        let mut pending_user: Option<String> = None;

        for (sender, message) in recent_messages {
            let prepared = Self::prepare_memory_text(&message, clip_chars);
            if prepared.is_empty() {
                continue;
            }

            if sender.eq_ignore_ascii_case("you") {
                if let Some(previous_user) = pending_user.replace(prepared) {
                    pairs.push((Some(previous_user), None));
                }
            } else if sender.eq_ignore_ascii_case("chatty") {
                pairs.push((pending_user.take(), Some(prepared)));
            }
        }

        if let Some(user_msg) = pending_user.take() {
            pairs.push((Some(user_msg), None));
        }

        if pairs.len() > max_pairs {
            pairs = pairs[pairs.len().saturating_sub(max_pairs)..].to_vec();
        }

        pairs
    }

    fn recent_chat_exchange_pairs(
        &self,
        clip_chars: Option<usize>,
        max_pairs: usize,
    ) -> Vec<String> {
        self.recent_chat_exchange_pair_entries(clip_chars, max_pairs)
            .into_iter()
            .filter_map(|(user_msg, chatty_msg)| match (user_msg, chatty_msg) {
                (Some(user_msg), Some(chatty_msg)) => Some(format!(
                    "Recent exchange: You said \"{user_msg}\". Chatty replied \"{chatty_msg}\"."
                )),
                (Some(user_msg), None) => {
                    Some(format!("Recent exchange: You said \"{user_msg}\"."))
                }
                (None, Some(chatty_msg)) => {
                    Some(format!("Recent exchange: Chatty replied \"{chatty_msg}\"."))
                }
                (None, None) => None,
            })
            .collect()
    }

    fn memory_jogger_items(&self) -> Vec<String> {
        self.memory_jogger
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| line.trim_start_matches("- ").trim().to_string())
            .collect()
    }

    fn render_chat_context_banner(&self, ui: &mut egui::Ui) {
        if let Some(assignment) = self.selected_assignment_ref() {
            egui::Frame::none()
                .fill(color_from_hex(&self.theme.surface))
                .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
                .rounding(egui::Rounding::same(6.0))
                .inner_margin(egui::vec2(10.0, 8.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Homework context active").strong());
                    ui.label(format!(
                        "{} ({}) | {} | Year {}",
                        assignment.title, assignment.id, assignment.subject, assignment.year_level
                    ));
                    if self.settings.homework_hints_only {
                        ui.label(
                            "Questions about this assignment will get hints, not full solutions.",
                        );
                    } else {
                        ui.label(
                            "If you ask about this assignment here, Chatty will use that homework context.",
                        );
                    }
                });
            ui.add_space(8.0);
        }
    }

    fn render_chat_log(&self, ui: &mut egui::Ui, log_height: f32) {
        ScrollArea::vertical()
            .id_source("main_chat_log_scroll")
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .max_height(log_height)
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                ui.set_min_height(log_height);
                let max_width = ui.available_width() * 0.96;
                ui.set_max_width(max_width);
                self.render_chat_messages(ui, &self.chat_log, max_width);
                ui.add_space(12.0);
            });
    }

    fn render_home_chat_preview(&self, ui: &mut egui::Ui) {
        let preview_start = self.chat_log.len().saturating_sub(4);
        let preview_messages = &self.chat_log[preview_start..];

        egui::Frame::none()
            .fill(color_from_hex(&self.theme.surface))
            .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::vec2(10.0, 8.0))
            .show(ui, |ui| {
                ui.set_min_height(148.0);
                if !self.chat_log.is_empty() && self.chat_log.len() > preview_messages.len() {
                    ui.label(
                        RichText::new("Showing the latest chat messages. Open Chat for the full conversation.")
                            .small()
                            .color(color_from_hex(&self.theme.muted_text)),
                    );
                    ui.add_space(4.0);
                }

                let max_width = ui.available_width() * 0.98;
                ui.set_max_width(max_width);
                self.render_chat_messages(ui, preview_messages, max_width);
            });
    }

    fn render_chat_messages(
        &self,
        ui: &mut egui::Ui,
        messages: &[(String, String)],
        max_width: f32,
    ) {
        if messages.is_empty() {
            ui.add_space(8.0);
            ui.label("No chat messages yet. Use the bottom chat bar to start a conversation.");
            return;
        }

        for (sender, msg) in messages {
            let is_user = sender.eq_ignore_ascii_case("you");
            let bubble_fill = if is_user {
                color_from_hex(&self.theme.accent_soft)
            } else {
                color_from_hex(&self.theme.surface)
            };
            let bubble_stroke = if is_user {
                color_from_hex(&self.theme.accent)
            } else {
                color_from_hex(&self.theme.border)
            };
            let text_color = if is_user {
                color_from_hex(&self.theme.accent)
            } else {
                color_from_hex(&self.theme.text)
            };
            let name_color = if is_user {
                bubble_stroke
            } else {
                color_from_hex(&self.theme.muted_text)
            };

            ui.add_space(4.0);
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.add_space(8.0);
                egui::Frame::none()
                    .fill(bubble_fill)
                    .stroke(egui::Stroke {
                        width: 1.0,
                        color: bubble_stroke,
                    })
                    .rounding(egui::Rounding::same(6.0))
                    .inner_margin(egui::vec2(10.0, 8.0))
                    .show(ui, |ui| {
                        ui.set_max_width(max_width * 0.9);
                        let safe_sender = Self::display_safe_text(sender);
                        let safe_message = Self::display_safe_text(msg);
                        ui.label(RichText::new(safe_sender).strong().color(name_color));
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(RichText::new(safe_message).color(text_color))
                                .wrap(true),
                        );
                    });
            });
        }
    }

    fn render_bookkeeper(&mut self, ui: &mut egui::Ui) {
        if !self.teacher_unlocked {
            ui.heading("Bookkeeper");
            ui.label("Unlock teacher view to open full session logs.");
            return;
        }

        let heading = ui.label(RichText::new("Bookkeeper logs").heading());
        let _ =
            heading.on_hover_text("Full session logs. Search past activity and diagnose issues.");
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Search");
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.bookkeeper_query)
                    .hint_text("Search chat text, dates, or homework labels"),
            );
            if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.refresh_bookkeeper_search();
            }
            if ui.button("Search logs").clicked() {
                self.refresh_bookkeeper_search();
            }
            if ui.button("Show recent").clicked() {
                self.bookkeeper_query.clear();
                self.refresh_bookkeeper_search();
            }
        });

        if let Some(status) = &self.bookkeeper_status {
            ui.add_space(4.0);
            ui.label(
                RichText::new(status)
                    .small()
                    .color(color_from_hex(&self.theme.muted_text)),
            );
        }
        ui.add_space(8.0);

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(false)
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                if self.bookkeeper_results.is_empty() {
                    ui.label("No cold log entries matched this search yet.");
                    return;
                }

                for entry in &self.bookkeeper_results {
                    egui::Frame::none()
                        .fill(color_from_hex(&self.theme.surface))
                        .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::vec2(10.0, 8.0))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(&entry.timestamp)
                                        .small()
                                        .color(color_from_hex(&self.theme.muted_text)),
                                );
                                ui.label(
                                    RichText::new(format!("{} | {}", entry.channel, entry.speaker))
                                        .small()
                                        .strong(),
                                );
                                ui.label(
                                    RichText::new(format!("session {}", entry.session_id))
                                        .small()
                                        .color(color_from_hex(&self.theme.muted_text)),
                                );
                            });
                            if let Some(note) = &entry.note {
                                if !note.trim().is_empty() {
                                    ui.label(
                                        RichText::new(note)
                                            .small()
                                            .color(color_from_hex(&self.theme.accent)),
                                    );
                                }
                            }
                            ui.add_space(4.0);
                            ui.label(entry.text.trim());
                        });
                    ui.add_space(8.0);
                }
            });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.label("Student profile");
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.settings.student.student_name);
        });
        ui.horizontal(|ui| {
            ui.label("Student ID");
            ui.text_edit_singleline(&mut self.settings.student.student_id);
        });
        ui.horizontal(|ui| {
            ui.label("Class ID");
            ui.text_edit_singleline(&mut self.settings.student.class_id);
        });
        ui.separator();
        ui.label(RichText::new("Model").strong());
        ui.horizontal(|ui| {
            ui.label("Max tokens");
            ui.add_enabled(
                self.teacher_unlocked,
                egui::DragValue::new(&mut self.settings.model.max_tokens).clamp_range(32..=4096),
            );
            ui.label("Higher = longer answers (slower).");
        });
        if !self.teacher_unlocked {
            ui.colored_label(
                self.warning_color(),
                "Unlock teacher view to change model limits.",
            );
        }
        ui.separator();
        if !self.teacher_unlocked {
            ui.label(RichText::new("Teacher controls").strong());
            ui.colored_label(
                self.warning_color(),
                "Teacher controls are locked. Unlock via the Teacher menu.",
            );
        } else {
            ui.label(RichText::new("Teacher controls").strong());
            ui.checkbox(
                &mut self.settings.janet.enabled,
                "Enable Janet safety filter",
            );
            ui.checkbox(
                &mut self.settings.janet.block_swears,
                "Block swears and rude words",
            );
            ui.checkbox(
                &mut self.settings.janet.block_mature_topics,
                "Block mature topics",
            );
            ui.separator();
            ui.checkbox(
                &mut self.settings.homework_hints_only,
                "Homework help gives hints only (no full answers)",
            );
            ui.separator();
            ui.checkbox(&mut self.settings.game.enabled, "Enable games");
            ui.checkbox(
                &mut self.settings.game.games_in_class_allowed,
                "Allow games in class",
            );
        }
        if ui.button("Save settings").clicked() {
            let _ = save_settings(&self.settings, &self.base_path);
            ui.label("Saved");
        }
    }

    fn render_diagnostics(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        let desired = if self.teacher_unlocked {
            DiagnosticsAudience::Teacher
        } else {
            DiagnosticsAudience::StudentSafe
        };
        if self.diagnostics_report.trim().is_empty() || self.diagnostics_audience != desired {
            self.refresh_diagnostics_report();
        }

        ui.heading("Diagnostic / Health Check");
        ui.label("Copy/paste this report when asking for support.");
        ui.label(format!(
            "Report type: {}",
            if self.teacher_unlocked {
                "teacher"
            } else {
                "student-safe"
            }
        ));

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.refresh_diagnostics_report();
            }
            if ui.button("Copy report").clicked() {
                if self.diagnostics_report.trim().is_empty() {
                    self.refresh_diagnostics_report();
                }
                ctx.copy_text(self.diagnostics_report.clone());
                self.diagnostics_status = Some("Copied report to clipboard.".to_string());
            }
            if let Some(msg) = &self.diagnostics_status {
                ui.colored_label(self.warning_color(), msg);
            }
        });

        ui.separator();
        let desired_rows = self.diagnostics_report.lines().count().max(24).min(4000);
        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .max_height(ui.available_height())
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.diagnostics_report)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(desired_rows)
                        .interactive(false),
                );
            });
    }

    fn normalize_model_message(text: &str) -> String {
        let stripped = Self::strip_prompt_template_markers(text);
        let mut lines = Vec::new();
        for line in stripped.lines() {
            let trimmed = Self::strip_prompt_role_prefixes(line.trim());
            if trimmed.is_empty() {
                continue;
            }
            lines.push(trimmed.to_string());
        }

        let joined = if lines.is_empty() {
            Self::strip_prompt_role_prefixes(stripped.trim()).to_string()
        } else {
            lines.join("\n")
        };
        joined.trim().to_string()
    }

    fn strip_prompt_template_markers(text: &str) -> String {
        let mut cleaned = text.to_string();

        for token in ["<s>", "</s>", "[INST]", "[/INST]", "<<SYS>>", "<</SYS>>"] {
            cleaned = cleaned.replace(token, " ");
        }

        let mut out = String::new();
        let mut remaining = cleaned.as_str();
        loop {
            if let Some(start) = remaining.find("<|") {
                out.push_str(&remaining[..start]);
                let after_start = &remaining[start + 2..];
                if let Some(end) = after_start.find("|>") {
                    remaining = &after_start[end + 2..];
                } else {
                    out.push_str(&remaining[start..]);
                    break;
                }
            } else {
                out.push_str(remaining);
                break;
            }
        }

        out
    }

    fn strip_prompt_role_prefixes(text: &str) -> &str {
        let mut current = text.trim();

        loop {
            let lower = current.to_ascii_lowercase();
            let mut advanced = false;

            for prefix in [
                "assistant:",
                "user:",
                "system:",
                "analysis:",
                "final:",
                "answer:",
            ] {
                if lower.starts_with(prefix) {
                    current = current[prefix.len()..].trim_start();
                    advanced = true;
                    break;
                }
            }

            if advanced {
                continue;
            }

            for prefix in ["assistant", "user", "system", "analysis", "final", "answer"] {
                if lower == prefix {
                    current = "";
                    advanced = true;
                    break;
                }
                let with_space = format!("{prefix} ");
                if lower.starts_with(&with_space) {
                    current = current[prefix.len()..].trim_start();
                    advanced = true;
                    break;
                }
            }

            if !advanced {
                break;
            }
        }

        current.trim()
    }

    fn prepare_memory_text(text: &str, clip_chars: Option<usize>) -> String {
        let normalized = Self::normalize_model_message(text);
        let collapsed = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
        let safe = Self::display_safe_text(&collapsed);
        if let Some(max_chars) = clip_chars {
            Self::clip_chars(&safe, max_chars)
        } else {
            safe
        }
    }

    fn clip_chars(text: &str, max_chars: usize) -> String {
        let mut out = String::new();
        for ch in text.chars().take(max_chars) {
            out.push(ch);
        }
        if text.chars().count() > max_chars {
            out.push_str("...");
        }
        out
    }

    fn display_safe_text(text: &str) -> String {
        let normalized = Self::normalize_common_mojibake(text);
        let mut out = String::new();
        for ch in normalized.chars() {
            match ch {
                '\r' => {}
                '\n' => out.push('\n'),
                '\t' | '\u{00A0}' => out.push(' '),
                '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' => {}
                c if c.is_whitespace() => out.push(' '),
                '\u{2018}' | '\u{2019}' => out.push('\''),
                '\u{201C}' | '\u{201D}' => out.push('"'),
                '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
                | '\u{2212}' => out.push('-'),
                '\u{2022}' => out.push('-'),
                '\u{2026}' => out.push_str("..."),
                '\u{2192}' => out.push_str("->"),
                '\u{2190}' => out.push_str("<-"),
                '│' | '┃' | '┆' | '┇' | '┊' | '┋' | '¦' | '｜' => out.push('|'),
                '─' | '━' | '═' | '﹘' | '﹣' | '－' => out.push('-'),
                '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' | '╭' | '╮' | '╰' | '╯'
                | '╳' | '╋' | '╂' | '╬' | '╠' | '╣' | '╦' | '╩' => out.push('+'),
                c if c.is_control() => {}
                c if c.is_ascii() => out.push(c),
                other => {
                    let transliterated = deunicode(&other.to_string());
                    if transliterated.trim().is_empty() {
                        out.push('?');
                    } else {
                        for fallback in transliterated.chars() {
                            match fallback {
                                '\r' => {}
                                '\n' => out.push('\n'),
                                '\t' => out.push(' '),
                                c if c.is_ascii_graphic() || c == ' ' => out.push(c),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        out.trim().to_string()
    }

    fn normalize_common_mojibake(text: &str) -> String {
        text.replace("â†’", "->")
            .replace("â†", "<-")
            .replace("â€”", "-")
            .replace("â€“", "-")
            .replace("â€˜", "'")
            .replace("â€™", "'")
            .replace("â€œ", "\"")
            .replace("â€", "\"")
            .replace("â€¦", "...")
            .replace("Â", " ")
    }

    fn warning_color(&self) -> egui::Color32 {
        if self.theme.name.eq_ignore_ascii_case("classic_light") {
            color_from_hex(&self.theme.accent)
        } else {
            egui::Color32::YELLOW
        }
    }

    fn render_teacher_homework_tools(&mut self, ui: &mut egui::Ui) {
        ui.heading("Teacher tools: Markdown homework");
        ui.label(
            "Workflow: write packs in .md (outgoing) → transcribe to JSON packs for students → submissions export to .json → convert to .md for marking.",
        );
        ui.label("Sharing: homework packs are simple files. You can share a pack .md or .json with other teachers/schools (include any referenced attachments).");

        let outgoing_dir = homework_markdown::homework_outgoing_dir(&self.base_path);
        let marking_dir = homework_markdown::homework_marking_dir(&self.base_path);
        let printables_dir = homework_markdown::homework_printables_dir(&self.base_path);
        let rubrics_dir = homework_markdown::homework_rubrics_dir(&self.base_path);
        ui.label(format!("Outgoing packs folder: {}", outgoing_dir.display()));
        ui.label(format!("Marking exports folder: {}", marking_dir.display()));
        ui.label(format!(
            "Student printables folder: {}",
            printables_dir.display()
        ));
        ui.label(format!("Teacher rubrics folder: {}", rubrics_dir.display()));

        ui.add(
            egui::TextEdit::multiline(&mut self.teacher_pack_request)
                .hint_text("Optional: Ask the local model to draft a homework pack (e.g., \"Year 7 Math fractions, 10 questions, include a word problem\").")
                .desired_width(f32::INFINITY)
                .desired_rows(3),
        );

        ui.horizontal(|ui| {
            if ui.button("Generate pack (.md) with AI").clicked() {
                let req = self.teacher_pack_request.trim().to_string();
                if req.is_empty() {
                    self.teacher_pack_status = Some("Type what you want the pack to include.".to_string());
                } else {
                    self.pulse_ecg(84.0, "Drafting a homework pack with the local model.");
                    let school_id = self
                        .current_pack
                        .as_ref()
                        .map(|p| p.school_id.clone())
                        .unwrap_or_else(|| "school".to_string());
                    let class_id = if self.settings.student.class_id.trim().is_empty() {
                        self.current_pack
                            .as_ref()
                            .map(|p| p.class_id.clone())
                            .unwrap_or_else(|| "class".to_string())
                    } else {
                        self.settings.student.class_id.trim().to_string()
                    };

                    let prompt = format!(
                        "{capsule}\nDefaults:\nversion: 1.0\nschool_id: {school}\nclass_id: {class}\ncreated_at: {now}\n\nTeacher request: {req}",
                        capsule = TEACHER_PACK_CAPSULE,
                        school = school_id,
                        class = class_id,
                        now = Utc::now().to_rfc3339(),
                        req = req
                    );

                    let result = panic::catch_unwind({
                        let mut model = self.settings.model.clone();
                        model.max_tokens = model.max_tokens.max(1024);
                        move || local_model::chat_completion(&model, &prompt)
                    });

                    match result {
                        Ok(Ok(text)) => {
                            self.teacher_pack_markdown = text.trim().to_string();
                            self.teacher_pack_status = if self.teacher_pack_markdown.is_empty() {
                                Some("The model returned an empty draft.".to_string())
                            } else {
                                Some(
                                    "Draft ready. Review/edit below, then save to outgoing."
                                        .to_string(),
                                )
                            };
                        }
                        Ok(Err(err)) => {
                            self.teacher_pack_status = Some(format!("AI generation failed: {err}"));
                        }
                        Err(_) => {
                            self.teacher_pack_status =
                                Some("Sorry, something went wrong while generating the pack.".to_string());
                        }
                    }
                }
            }

            let can_save = !self.teacher_pack_markdown.trim().is_empty();
            if ui
                .add_enabled(can_save, egui::Button::new("Save draft to outgoing"))
                .clicked()
            {
                self.pulse_ecg(42.0, "Saving a homework pack draft to outgoing.");
                let class_id = sanitize_filename_component(&self.settings.student.class_id);
                let ts = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
                let base_name = if class_id.trim().is_empty() {
                    format!("homework_pack_{ts}")
                } else {
                    format!("homework_pack_{class_id}_{ts}")
                };
                let _ = fs::create_dir_all(&outgoing_dir);
                let mut out_path = outgoing_dir.join(format!("{base_name}.md"));
                let mut n = 1usize;
                while out_path.exists() {
                    out_path = outgoing_dir.join(format!("{base_name}_{n}.md"));
                    n += 1;
                }
                match fs::write(&out_path, format!("{}\n", self.teacher_pack_markdown.trim_end()))
                {
                    Ok(_) => {
                        self.teacher_tools_status =
                            Some(format!("Saved outgoing pack: {}", out_path.display()));
                    }
                    Err(err) => {
                        self.teacher_tools_status =
                            Some(format!("Failed to save outgoing pack: {err}"));
                    }
                }
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Transcribe outgoing (.md → .json)").clicked() {
                self.pulse_ecg(66.0, "Transcribing outgoing homework markdown into JSON.");
                let defaults = PackMdDefaults {
                    version: "1.0".to_string(),
                    school_id: self
                        .current_pack
                        .as_ref()
                        .map(|p| p.school_id.clone())
                        .unwrap_or_else(|| "school".to_string()),
                    class_id: if self.settings.student.class_id.trim().is_empty() {
                        self.current_pack
                            .as_ref()
                            .map(|p| p.class_id.clone())
                            .unwrap_or_else(|| "class".to_string())
                    } else {
                        self.settings.student.class_id.trim().to_string()
                    },
                };

                match homework_markdown::transcribe_outgoing_packs(&self.base_path, &defaults) {
                    Ok(report) => {
                        self.teacher_tools_status = Some(format!(
                            "Outgoing → JSON: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        ));
                        if let Some(first) = report.errors.first() {
                            self.teacher_tools_status = Some(format!(
                                "{}. First error: {}",
                                self.teacher_tools_status.clone().unwrap_or_default(),
                                first
                            ));
                        }
                        self.resync_homework();
                        if let Some(pack) = self.current_pack.clone() {
                            apply_pack_policy(&mut self.settings, &pack);
                            let _ = save_settings(&self.settings, &self.base_path);
                        }
                    }
                    Err(err) => {
                        self.teacher_tools_status =
                            Some(format!("Outgoing → JSON failed: {err}"));
                    }
                }
            }

            if ui.button("Convert submissions (.json → .md)").clicked() {
                self.pulse_ecg(58.0, "Converting student submissions into marking markdown.");
                match homework_markdown::transcribe_completed_submissions_to_marking_md(
                    &self.base_path,
                    self.current_pack.as_ref(),
                ) {
                    Ok(report) => {
                        self.teacher_tools_status = Some(format!(
                            "Submissions → marking .md: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        ));
                        if let Some(first) = report.errors.first() {
                            self.teacher_tools_status = Some(format!(
                                "{}. First error: {}",
                                self.teacher_tools_status.clone().unwrap_or_default(),
                                first
                            ));
                        }
                    }
                    Err(err) => {
                        self.teacher_tools_status =
                            Some(format!("Submissions → marking .md failed: {err}"));
                    }
                }
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Export student printables (.md)").clicked() {
                let Some(pack) = self.current_pack.clone() else {
                    self.teacher_tools_status = Some(
                        "No pack loaded. Import/transcribe a pack first, then export.".to_string(),
                    );
                    return;
                };

                self.pulse_ecg(46.0, "Exporting student printables.");
                match homework_markdown::export_student_printables(&self.base_path, &pack) {
                    Ok(report) => {
                        self.teacher_tools_status = Some(format!(
                            "Student printables: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        ));
                        if let Some(first) = report.errors.first() {
                            self.teacher_tools_status = Some(format!(
                                "{}. First error: {}",
                                self.teacher_tools_status.clone().unwrap_or_default(),
                                first
                            ));
                        }
                    }
                    Err(err) => {
                        self.teacher_tools_status =
                            Some(format!("Student printables export failed: {err}"));
                    }
                }
            }

            if ui.button("Export teacher rubrics (.md)").clicked() {
                let Some(pack) = self.current_pack.clone() else {
                    self.teacher_tools_status = Some(
                        "No pack loaded. Import/transcribe a pack first, then export.".to_string(),
                    );
                    return;
                };

                self.pulse_ecg(46.0, "Exporting teacher rubrics.");
                match homework_markdown::export_teacher_rubrics(&self.base_path, &pack) {
                    Ok(report) => {
                        self.teacher_tools_status = Some(format!(
                            "Teacher rubrics: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        ));
                        if let Some(first) = report.errors.first() {
                            self.teacher_tools_status = Some(format!(
                                "{}. First error: {}",
                                self.teacher_tools_status.clone().unwrap_or_default(),
                                first
                            ));
                        }
                    }
                    Err(err) => {
                        self.teacher_tools_status =
                            Some(format!("Teacher rubrics export failed: {err}"));
                    }
                }
            }
        });

        if let Some(status) = &self.teacher_pack_status {
            ui.colored_label(self.warning_color(), status);
        }
        if let Some(status) = &self.teacher_tools_status {
            ui.colored_label(self.warning_color(), status);
        }

        if !self.teacher_pack_markdown.trim().is_empty() {
            ui.separator();
            ui.label(RichText::new("Pack draft (.md)").strong());
            ui.add(
                egui::TextEdit::multiline(&mut self.teacher_pack_markdown)
                    .desired_width(f32::INFINITY)
                    .desired_rows(14),
            );
        }
    }

    fn render_teacher_revision_tools(&mut self, ui: &mut egui::Ui) {
        ui.heading("Teacher tools: Revision");
        ui.label(
            "Revision is separate from live homework. It reads from completed homework submissions and stores teacher revision materials under revision/.",
        );
        let revision_root = revision_dir(&self.base_path);
        let past_papers_root = revision_past_papers_dir(&self.base_path);
        ui.label(format!("Revision workspace: {}", revision_root.display()));
        ui.label(format!(
            "Past papers folder: {}",
            past_papers_root.display()
        ));
        ui.label(format!(
            "Completed submissions available to revision: {}",
            self.revision_sources.len()
        ));
        ui.label(format!("Imported past papers: {}", self.past_papers.len()));

        ui.horizontal(|ui| {
            if ui.button("Open Revision").clicked() {
                self.open_revision_workspace();
            }
            if ui
                .add_enabled(
                    !self.revision_sources.is_empty(),
                    egui::Button::new("Create revision pack (.md)"),
                )
                .clicked()
            {
                self.pulse_ecg(44.0, "Building a revision pack from completed homework.");
                match build_revision_pack_markdown(&self.base_path, &self.revision_sources) {
                    Ok(path) => {
                        self.teacher_revision_status =
                            Some(format!("Revision pack written to {}", path.display()));
                    }
                    Err(err) => {
                        self.teacher_revision_status =
                            Some(format!("Revision pack export failed: {err}"));
                    }
                }
            }
            if ui.button("Import past paper...").clicked() {
                if let Some(file) = FileDialog::new().pick_file() {
                    match import_past_paper(&self.base_path, &file) {
                        Ok(path) => {
                            self.resync_revision();
                            self.teacher_revision_status =
                                Some(format!("Past paper copied to {}", path.display()));
                        }
                        Err(err) => {
                            self.teacher_revision_status =
                                Some(format!("Past paper import failed: {err}"));
                        }
                    }
                }
            }
        });

        if !self.past_papers.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Past papers found").strong());
            for path in self.past_papers.iter().take(8) {
                ui.label(path.display().to_string());
            }
        }

        if let Some(status) = &self.teacher_revision_status {
            ui.colored_label(self.warning_color(), status);
        }
    }

    fn render_homework_dashboard(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical()
            .id_source("homework_dashboard_scroll")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.heading("Homework dashboard");
                ui.horizontal(|ui| {
                    if ui.button("Diagnostic / Health Check").clicked() {
                        self.open_diagnostics_tab();
                    }
                });
                if let Some(pack) = self.current_pack.clone() {
                    let visible_assignments = Self::unique_assignments_by_id(&pack);
                    ui.label(format!(
                        "Class: {} | Assignments: {}",
                        pack.class_id,
                        visible_assignments.len()
                    ));
                } else {
                    ui.label("No pack loaded yet. Import a pack to see class metrics.");
                }

                ui.separator();
                self.render_teacher_homework_tools(ui);
                ui.separator();
                self.render_teacher_revision_tools(ui);
                ui.separator();

                let all_entries = self.score_entries();
                let focused_entries: Vec<StudentScore> = if self.selected_students.is_empty() {
                    all_entries.clone()
                } else {
                    all_entries
                        .into_iter()
                        .filter(|s| self.selected_students.contains(&s.student_name))
                        .collect()
                };

                if focused_entries.is_empty() {
                    ui.label("No submissions found yet.");
                } else {
                    let (class_avg, per_student_avg, per_subject_avg) =
                        aggregate_scores(&focused_entries);
                    ui.separator();
                    ui.label("Class / selection average");
                    ui.add(
                        ProgressBar::new(class_avg / 100.0)
                            .fill(score_color(class_avg))
                            .text(format!("{:.1} / 100", class_avg)),
                    );

                    ui.horizontal(|ui| {
                        ui.label("Students:");
                        if ui.button("Clear selection").clicked() {
                            self.selected_students.clear();
                        }
                    });
                    ScrollArea::vertical()
                        .id_source("homework_dashboard_students_scroll")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for (name, avg) in &per_student_avg {
                                ui.push_id(("homework_dashboard_student", name), |ui| {
                                    let selected = self.selected_students.contains(name);
                                    let label = format!("{name} ({avg:.1})");
                                    if ui.selectable_label(selected, label).clicked() {
                                        if selected {
                                            self.selected_students.remove(name);
                                        } else {
                                            self.selected_students.insert(name.clone());
                                        }
                                    }
                                });
                            }
                        });

                    ui.separator();
                    ui.label("Subject metrics");
                    for (subj, score) in &per_subject_avg {
                        ui.horizontal(|ui| {
                            ui.label(subj);
                            ui.add(
                                ProgressBar::new(*score / 100.0)
                                    .fill(score_color(*score))
                                    .text(format!("{score:.1}")),
                            );
                        });
                    }
                }

                if !self.submissions.is_empty() {
                    ui.separator();
                    ui.heading("Submissions found locally");
                    ScrollArea::vertical()
                        .id_source("homework_dashboard_submissions_scroll")
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for row in self.submission_rows() {
                                ui.push_id(
                                    (
                                        "homework_dashboard_submission_row",
                                        &row.assignment_id,
                                        &row.student_id,
                                        &row.submitted_at,
                                    ),
                                    |ui| {
                                        let label = format!(
                                            "{} ({}) - {} ({}) | subj: {} | score: {} | {}",
                                            row.assignment_title,
                                            row.assignment_id,
                                            row.student_name,
                                            row.student_id,
                                            row.subject,
                                            row.score,
                                            row.feedback
                                        );
                                        ui.label(label).on_hover_text(format!(
                                            "Assignment ID: {} | Student ID: {} | Submitted: {}",
                                            row.assignment_id, row.student_id, row.submitted_at
                                        ));
                                    },
                                );
                            }
                        });
                }
            });
    }

    fn build_revision_prompt(&self, source: &RevisionSource, user_input: &str) -> String {
        let notes = if self.revision_notes.trim().is_empty() {
            "No saved revision notes yet.".to_string()
        } else {
            self.revision_notes.trim().to_string()
        };
        let instructions = source
            .instructions_md
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or("No assignment snapshot stored in this submission.");
        let feedback = source
            .ai_feedback
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or("No revision focus note recorded.");

        format!(
            "{capsule}\nRevision source:\n- Assignment: {title} ({id})\n- Subject: {subject}\n- Year level: {year}\n- Submitted: {submitted}\n\nInternal guidance (do not quote these notes back to the student as diagnostics):\n- Revision focus note: {feedback}\n\nOriginal assignment snapshot:\n{instructions}\n\nStudent's previous submission:\n{submission}\n\nSaved revision notes:\n{notes}\n\nStudent request: {request}\nRespond with one short revision-focused reply.",
            capsule = REVISION_CHAT_CAPSULE,
            title = source.assignment_title.as_str(),
            id = source.assignment_id.as_str(),
            subject = source.subject.as_str(),
            year = source.year_level.as_str(),
            submitted = source.submitted_at.as_str(),
            feedback = feedback,
            instructions = instructions,
            submission = if source.answers_text.trim().is_empty() {
                "No submission text stored."
            } else {
                source.answers_text.trim()
            },
            notes = notes,
            request = user_input
        )
    }

    fn revision_confidence_label(confidence: i32) -> &'static str {
        match confidence.clamp(0, 100) {
            0..=30 => "Need more practice",
            31..=55 => "Getting there",
            56..=80 => "Mostly comfortable",
            _ => "Ready to move on",
        }
    }

    fn revision_confidence_bucket(confidence: i32) -> i32 {
        match confidence.clamp(0, 100) {
            0..=30 => 0,
            31..=55 => 1,
            56..=80 => 2,
            _ => 3,
        }
    }

    fn revision_source_label(source: &RevisionSource, teacher_unlocked: bool) -> String {
        if teacher_unlocked {
            format!(
                "{} ({}) | score {}",
                source.assignment_title,
                source.assignment_id,
                source
                    .ai_score
                    .map(|score| score.to_string())
                    .unwrap_or_else(|| "-".to_string())
            )
        } else {
            format!("{} ({})", source.assignment_title, source.assignment_id)
        }
    }

    fn revision_social_cue_reply(text: &str) -> Option<&'static str> {
        let normalized: String = text
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        match normalized.as_str() {
            "thanks" | "thank you" | "thankyou" | "thanks chatty" | "thank you chatty"
            | "got it" | "ok thanks" | "okay thanks" | "cheers" | "bye" | "goodbye" => {
                Some("You're welcome. Ask if you want to revise one more part.")
            }
            _ => None,
        }
    }

    fn handle_revision_send(&mut self) {
        if self.revision_chat_input.trim().is_empty() {
            return;
        }
        let Some(source) = self.selected_revision_source().cloned() else {
            return;
        };

        let user_msg = self.revision_chat_input.trim().to_string();
        if let Some(reply) = Self::revision_social_cue_reply(&user_msg) {
            self.revision_chat_log
                .push(("You".to_string(), user_msg.clone()));
            self.revision_chat_log
                .push(("Chatty".to_string(), reply.to_string()));
            self.revision_chat_input.clear();
            self.pulse_ecg(8.0, "Handled a short revision social cue locally.");
            return;
        }

        self.pulse_ecg(68.0, "Generating revision help with the local model.");
        self.revision_chat_log
            .push(("You".to_string(), user_msg.clone()));
        self.revision_chat_log
            .push(("Chatty".to_string(), "...".to_string()));

        let result = panic::catch_unwind({
            let settings = self.settings.clone();
            let prompt = self.build_revision_prompt(&source, &user_msg);
            move || generate_answer(&settings, &prompt)
        });

        let reply = match result {
            Ok(text) => Self::normalize_model_message(&text),
            Err(_) => "Sorry, I ran into an error while answering.".to_string(),
        };

        if let Some(last) = self.revision_chat_log.last_mut() {
            last.1 = reply;
        }
        self.revision_chat_input.clear();
    }

    fn render_revision_workspace(&mut self, ui: &mut egui::Ui) {
        ui.heading("Revision");
        ui.label(
            "Revision is separate from live homework. It uses completed homework as source material and stores notes or progress under revision/.",
        );
        if ui.button("Diagnostic / Health Check").clicked() {
            self.open_diagnostics_tab();
        }

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                ui.separator();
                ui.label(format!(
                    "Completed homework sources: {} | Past papers: {}",
                    self.revision_sources.len(),
                    self.past_papers.len()
                ));
                if self.teacher_unlocked {
                    ui.label(
                        "Spaced repetition view: lower scores, saved low confidence, and feedback-heavy items naturally rise to the top.",
                    );
                } else {
                    ui.label(
                        "Revision gently lifts the topics you seem to need most, so the right items stay near the top.",
                    );
                }

                if self.revision_sources.is_empty() {
                    ui.colored_label(
                        self.warning_color(),
                        "No completed homework was found for revision yet. Submit homework from Home first, or add past papers below.",
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Revision source:");
                        let revision_choices = self.revision_sources.clone();
                        let current = self
                            .selected_revision_source()
                            .map(|source| Self::revision_source_label(source, self.teacher_unlocked))
                            .unwrap_or_else(|| "Select revision source".to_string());
                        egui::ComboBox::from_id_source("revision_source_select")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for source in &revision_choices {
                                    let label =
                                        Self::revision_source_label(source, self.teacher_unlocked);
                                    if ui
                                        .selectable_label(
                                            self.selected_revision.as_ref()
                                                == Some(&source.revision_key),
                                            label,
                                        )
                                        .clicked()
                                    {
                                        self.selected_revision =
                                            Some(source.revision_key.clone());
                                        self.revision_chat_log.clear();
                                        self.sync_revision_editor_from_selection();
                                    }
                                }
                            });
                    });

                    if let Some(source) = self.selected_revision_source().cloned() {
                        let saved_progress =
                            self.revision_progress.get(&source.revision_key).cloned();
                        ui.separator();
                        ui.label(
                            RichText::new(&source.assignment_title)
                                .heading()
                                .color(color_from_hex(&self.theme.accent)),
                        );
                        ui.label(format!(
                            "{} | Year {} | Submitted {}",
                            source.subject, source.year_level, source.submitted_at
                        ));
                        if self.teacher_unlocked {
                            ui.label(format!(
                                "Revision priority: {} | AI score: {}",
                                revision_priority(&source),
                                source
                                    .ai_score
                                    .map(|score| score.to_string())
                                    .unwrap_or_else(|| "-".to_string())
                            ));
                        }
                        if let Some(progress) = &saved_progress {
                            if self.teacher_unlocked {
                                ui.label(format!(
                                    "Saved confidence: {} / 100 | Reviews logged: {}",
                                    progress.confidence, progress.review_count
                                ));
                            } else {
                                ui.label(format!(
                                    "Confidence check-in: {}",
                                    Self::revision_confidence_label(progress.confidence)
                                ));
                            }
                            if !progress.last_reviewed_at.trim().is_empty() {
                                ui.label(format!(
                                    "Last reviewed: {}",
                                    progress.last_reviewed_at
                                ));
                            }
                        }
                        if self.teacher_unlocked {
                            if let Some(feedback) = source.ai_feedback.as_deref() {
                                ui.colored_label(
                                    self.warning_color(),
                                    format!("Focus from feedback: {}", feedback.trim()),
                                );
                            }
                        }
                        if let Some(instructions) = source.instructions_md.as_deref() {
                            self.render_markdown_card(
                                ui,
                                "Original assignment snapshot",
                                instructions,
                            );
                        } else if self.teacher_unlocked {
                            ui.colored_label(
                                self.warning_color(),
                                "This older submission does not include an embedded assignment snapshot, so revision is based on the submission and feedback only.",
                            );
                        }
                        else {
                            ui.colored_label(
                                self.warning_color(),
                                "This older submission does not include the original assignment snapshot, so revision is based on your submission only.",
                            );
                        }

                        if !source.answers_text.trim().is_empty() {
                            ui.add_space(6.0);
                            self.render_markdown_card(
                                ui,
                                "Your submitted work",
                                &source.answers_text,
                            );
                        }

                        ui.separator();
                        ui.heading("Revision notes");
                        if self.teacher_unlocked {
                            ui.add(
                                egui::Slider::new(&mut self.revision_confidence, 0..=100)
                                    .text("Confidence"),
                            );
                        } else {
                            ui.label("How ready do you feel about this topic?");
                            let selected_bucket =
                                Self::revision_confidence_bucket(self.revision_confidence);
                            ui.horizontal_wrapped(|ui| {
                                for (bucket, label, value) in [
                                    (0, "Need more practice", 20),
                                    (1, "Getting there", 45),
                                    (2, "Mostly comfortable", 70),
                                    (3, "Ready to move on", 90),
                                ] {
                                    let selected = selected_bucket == bucket;
                                    if ui.selectable_label(selected, label).clicked() {
                                        self.revision_confidence = value;
                                    }
                                }
                            });
                        }
                        ui.add(
                            egui::TextEdit::multiline(&mut self.revision_notes)
                                .desired_width(f32::INFINITY)
                                .desired_rows(6)
                                .hint_text("Write what you need to revisit, rules to remember, or your own summary."),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Save revision notes").clicked() {
                                let mut review_count = saved_progress
                                    .as_ref()
                                    .map(|progress| progress.review_count)
                                    .unwrap_or(0);
                                review_count += 1;
                                let progress = RevisionProgress {
                                    revision_key: source.revision_key.clone(),
                                    notes: self.revision_notes.trim().to_string(),
                                    confidence: self.revision_confidence.clamp(0, 100),
                                    review_count,
                                    last_reviewed_at: Utc::now().to_rfc3339(),
                                };
                                match save_revision_progress(&self.base_path, &progress) {
                                    Ok(path) => {
                                        self.revision_progress.insert(
                                            progress.revision_key.clone(),
                                            progress,
                                        );
                                        self.resync_revision();
                                        self.revision_status = Some(format!(
                                            "Saved revision notes to {}",
                                            path.display()
                                        ));
                                        self.pulse_ecg(22.0, "Saved revision notes.");
                                    }
                                    Err(err) => {
                                        self.revision_status = Some(format!(
                                            "Revision save failed: {err}"
                                        ));
                                    }
                                }
                            }
                            if ui.button("Reload saved notes").clicked() {
                                self.sync_revision_editor_from_selection();
                                self.revision_status =
                                    Some("Reloaded saved revision notes.".to_string());
                            }
                            if ui.button("Clear draft").clicked() {
                                self.revision_notes.clear();
                                self.revision_confidence = 50;
                            }
                        });
                        if let Some(status) = &self.revision_status {
                            ui.label(status);
                        }

                        ui.separator();
                        ui.heading("Revision helper");
                        ui.label(
                            "Open helper mode: this work is already submitted, so Chatty can explain more directly while staying focused on learning.",
                        );
                        egui::Frame::none()
                            .fill(color_from_hex(&self.theme.surface))
                            .stroke(egui::Stroke::new(
                                1.0,
                                color_from_hex(&self.theme.border),
                            ))
                            .rounding(egui::Rounding::same(6.0))
                            .inner_margin(egui::vec2(10.0, 8.0))
                            .show(ui, |ui| {
                                ui.set_min_height(160.0);
                                let max_width = ui.available_width() * 0.98;
                                ui.set_max_width(max_width);
                                self.render_chat_messages(
                                    ui,
                                    &self.revision_chat_log,
                                    max_width,
                                );
                            });
                        ui.horizontal(|ui| {
                            let input = ui.add(
                                egui::TextEdit::singleline(&mut self.revision_chat_input)
                                    .hint_text("Ask about this completed work, a mistake, or how to revise it..."),
                            );
                            if input.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                self.handle_revision_send();
                            }
                            if ui.button("Send").clicked() {
                                self.handle_revision_send();
                            }
                        });
                    }
                }

                if !self.past_papers.is_empty() {
                    ui.separator();
                    ui.heading("Past papers");
                    for path in &self.past_papers {
                        ui.label(path.display().to_string());
                    }
                }
            });
    }

    fn render_module_tab(&mut self, ui: &mut egui::Ui, tab_idx: usize) {
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        let TabKind::Module {
            module,
            cached_text,
        } = &mut tab.kind
        else {
            return;
        };

        if module.manifest.id == "homework_dashboard" && !self.teacher_unlocked {
            ui.colored_label(
                self.warning_color(),
                "Teacher view is locked. Unlock via the Teacher menu to open this dashboard.",
            );
            return;
        }

        ui.heading(&module.manifest.title);
        if let Some(desc) = &module.manifest.description {
            ui.label(desc);
        }
        ui.separator();

        match &module.manifest.entry {
            ModuleEntry::BuiltinPanel { target } => match target.as_str() {
                "homework_dashboard" => self.render_homework_dashboard(ui),
                "homework_assignments" | "revision_workspace" => self.render_revision_workspace(ui),
                _ => {
                    ui.label(format!("Builtin panel stub: {}", target));
                }
            },
            ModuleEntry::Markdown { path } => {
                if cached_text.is_none() {
                    let full_path = module.folder.join(path);
                    *cached_text = fs::read_to_string(&full_path).ok();
                }
                if let Some(text) = cached_text {
                    render_markdown(ui, text);
                } else {
                    ui.label("Could not load markdown file.");
                }
            }
            ModuleEntry::StaticHtml { path } => {
                ui.label(format!("Static HTML module (not rendered yet): {}", path));
            }
            ModuleEntry::ExternalProcess { command, args } => {
                if self.allow_external_process {
                    ui.label(format!(
                        "External process would run: {} {:?}",
                        command, args
                    ));
                    ui.label("Process launching is stubbed for safety.");
                } else {
                    ui.colored_label(
                        self.warning_color(),
                        "External processes are disabled in safe mode.",
                    );
                }
            }
        }
    }

    fn selected_assignment_ref(&self) -> Option<&HomeworkAssignment> {
        let pack = self.current_pack.as_ref()?;
        let unique_assignments = Self::unique_assignments_by_id(pack);
        if let Some(id) = &self.selected_assignment {
            if let Some(found) = unique_assignments.iter().copied().find(|a| &a.id == id) {
                return Some(found);
            }
        }
        unique_assignments.first().copied()
    }

    fn selected_revision_source(&self) -> Option<&RevisionSource> {
        if let Some(key) = &self.selected_revision {
            if let Some(found) = self
                .revision_sources
                .iter()
                .find(|source| &source.revision_key == key)
            {
                return Some(found);
            }
        }
        self.revision_sources.first()
    }

    fn clean_markdown_fences(text: &str) -> String {
        text.lines()
            .filter(|line| !line.trim_start().starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    fn normalize_compare_text(text: &str) -> String {
        Self::clean_markdown_fences(text)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    }

    fn normalize_homework_match_text(text: &str) -> String {
        let mut out = String::new();
        let mut previous_space = false;

        for ch in text.to_ascii_lowercase().chars() {
            let mapped = if ch.is_ascii_alphanumeric()
                || matches!(ch, '+' | '-' | '*' | '/' | '=' | '^' | '.')
            {
                ch
            } else {
                ' '
            };

            if mapped == ' ' {
                if !previous_space {
                    out.push(' ');
                    previous_space = true;
                }
            } else {
                out.push(mapped);
                previous_space = false;
            }
        }

        out.trim().to_string()
    }

    fn homework_match_tokens(text: &str) -> Vec<String> {
        Self::normalize_homework_match_text(text)
            .split_whitespace()
            .map(|token| token.trim_matches('.'))
            .filter(|token| !token.is_empty())
            .map(|token| token.to_string())
            .collect()
    }

    fn is_homework_stopword(token: &str) -> bool {
        matches!(
            token,
            "a" | "an"
                | "and"
                | "are"
                | "be"
                | "by"
                | "for"
                | "from"
                | "how"
                | "i"
                | "in"
                | "is"
                | "it"
                | "me"
                | "my"
                | "of"
                | "on"
                | "or"
                | "please"
                | "question"
                | "solve"
                | "the"
                | "this"
                | "to"
                | "what"
                | "with"
                | "work"
                | "out"
        )
    }

    fn homework_keyword_tokens(tokens: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for token in tokens {
            let keep = token.chars().any(|ch| ch.is_ascii_digit())
                || token
                    .chars()
                    .any(|ch| matches!(ch, '+' | '-' | '*' | '/' | '=' | '^'))
                || token.len() >= 3;
            if keep && !Self::is_homework_stopword(token) && !out.contains(token) {
                out.push(token.clone());
            }
        }
        out
    }

    fn homework_number_tokens(tokens: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for token in tokens {
            if token.chars().any(|ch| ch.is_ascii_digit()) && !out.contains(token) {
                out.push(token.clone());
            }
        }
        out
    }

    fn homework_signature_phrases(normalized_question: &str, tokens: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        if normalized_question.len() >= 6 {
            out.push(normalized_question.to_string());
        }

        let max_window = tokens.len().min(4);
        for window_size in 2..=max_window {
            for window in tokens.windows(window_size) {
                if window.iter().all(|token| Self::is_homework_stopword(token)) {
                    continue;
                }
                let phrase = window.join(" ");
                if phrase.len() >= 8 && !out.contains(&phrase) {
                    out.push(phrase);
                }
            }
        }

        out
    }

    fn is_assignment_metadata_line(text: &str) -> bool {
        let Some((key, _)) = text.split_once(':') else {
            return false;
        };
        let normalized = key
            .trim()
            .to_ascii_lowercase()
            .replace(' ', "_")
            .replace('-', "_");
        matches!(
            normalized.as_str(),
            "assignment"
                | "attachments"
                | "allow_ai_premark"
                | "allow_games"
                | "class"
                | "due"
                | "due_at"
                | "grade"
                | "grade_level"
                | "gradelevel"
                | "max_score"
                | "school"
                | "student_id"
                | "student_name"
                | "subject"
                | "version"
                | "year"
                | "year_group"
                | "year_level"
                | "yeargroup"
                | "yearlevel"
        )
    }

    fn clean_question_candidate(text: &str) -> String {
        text.trim()
            .trim_matches(|ch: char| matches!(ch, '-' | '*' | '+' | ' '))
            .trim_end_matches(':')
            .trim()
            .to_string()
    }

    fn looks_like_question_candidate(text: &str, from_list_item: bool) -> bool {
        let cleaned = Self::clean_question_candidate(text);
        if cleaned.len() < 5 || Self::is_assignment_metadata_line(&cleaned) {
            return false;
        }

        let lower = cleaned.to_ascii_lowercase();
        if lower.starts_with("student name") || lower.starts_with("student id") {
            return false;
        }
        if cleaned.ends_with('?') {
            return true;
        }
        if [
            "add",
            "calculate",
            "compare",
            "complete",
            "convert",
            "describe",
            "determine",
            "draw",
            "evaluate",
            "explain",
            "find",
            "graph",
            "identify",
            "list",
            "plot",
            "show",
            "simplify",
            "solve",
            "sum",
            "what",
            "which",
            "work out",
            "write",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
        {
            return true;
        }

        let has_digit = cleaned.chars().any(|ch| ch.is_ascii_digit());
        let has_math = cleaned
            .chars()
            .any(|ch| matches!(ch, '+' | '-' | '*' | '/' | '=' | '^'));
        if has_digit && has_math {
            return true;
        }

        from_list_item && cleaned.split_whitespace().count() >= 4
    }

    fn extract_question_candidates(text: &str) -> Vec<(Option<usize>, String)> {
        let mut out = Vec::new();

        for raw_line in Self::clean_markdown_fences(text).lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some((number, rest)) = Self::parse_numbered_instruction_line(trimmed) {
                if Self::looks_like_question_candidate(rest, true) {
                    out.push((Some(number), Self::clean_question_candidate(rest)));
                }
                continue;
            }

            let bullet = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| trimmed.strip_prefix("+ "));
            let candidate = bullet.unwrap_or(trimmed);
            if Self::looks_like_question_candidate(candidate, bullet.is_some()) {
                out.push((None, Self::clean_question_candidate(candidate)));
            }
        }

        out
    }

    fn summarize_text(text: &str, max_lines: usize, max_chars: usize) -> String {
        let mut out = String::new();
        for (idx, line) in text.lines().enumerate() {
            if idx >= max_lines {
                break;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line.trim_end());
        }
        let trimmed = out.trim().to_string();
        if trimmed.len() > max_chars {
            let mut shortened = trimmed;
            shortened.truncate(max_chars);
            shortened
        } else {
            trimmed
        }
    }

    fn assignment_printable_display(&self, assignment: &HomeworkAssignment) -> Option<String> {
        let printable = assignment.student_printable_md.as_deref()?;
        let cleaned = Self::clean_markdown_fences(printable);
        if cleaned.is_empty()
            || Self::normalize_compare_text(&cleaned)
                == Self::normalize_compare_text(&assignment.instructions_md)
        {
            None
        } else {
            Some(cleaned)
        }
    }

    fn assignment_resource_warning(&self, assignment: &HomeworkAssignment) -> Option<String> {
        let lower = assignment.instructions_md.to_ascii_lowercase();
        let mentions_extra_resource = [
            "attached",
            "attachment",
            "worksheet",
            "field notes",
            "table",
            "chart",
            "graph",
            "list",
        ]
        .iter()
        .any(|needle| lower.contains(needle));

        if mentions_extra_resource
            && assignment.attachments.is_empty()
            && self.assignment_printable_display(assignment).is_none()
        {
            Some(
                "This task seems to refer to an extra worksheet/list/resource, but none is visible in this pack."
                    .to_string(),
            )
        } else {
            None
        }
    }

    fn render_markdown_card(&self, ui: &mut egui::Ui, title: &str, body: &str) {
        ui.label(RichText::new(title).strong());
        egui::Frame::none()
            .fill(color_from_hex(&self.theme.surface))
            .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::vec2(10.0, 8.0))
            .show(ui, |ui| {
                render_markdown(ui, body);
            });
    }

    fn resolve_assignment_attachment_path(&self, attachment: &str) -> Option<PathBuf> {
        let trimmed = attachment.trim();
        if trimmed.is_empty() {
            return None;
        }

        let as_path = PathBuf::from(trimmed);
        let mut candidates = Vec::new();
        if as_path.is_absolute() {
            candidates.push(as_path.clone());
        }
        candidates.push(self.base_path.join(trimmed));
        candidates.push(
            self.base_path
                .join("homework")
                .join("assigned")
                .join(trimmed),
        );

        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join(trimmed));
            candidates.push(cwd.join("resources").join(trimmed));
        }

        candidates.into_iter().find(|path| path.exists())
    }

    fn load_text_attachment_preview(
        &self,
        attachment: &str,
        max_lines: usize,
        max_chars: usize,
    ) -> Option<String> {
        let path = self.resolve_assignment_attachment_path(attachment)?;
        let is_text_like = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "txt" | "md" | "markdown" | "csv" | "json" | "log" | "tsv"
                )
            })
            .unwrap_or(true);
        if !is_text_like {
            return None;
        }

        let contents = fs::read_to_string(path).ok()?;
        let preview = Self::summarize_text(&contents, max_lines, max_chars);
        if preview.trim().is_empty() {
            None
        } else {
            Some(preview)
        }
    }

    fn render_assignment_materials(&self, ui: &mut egui::Ui, assignment: &HomeworkAssignment) {
        if let Some(warning) = self.assignment_resource_warning(assignment) {
            ui.colored_label(self.warning_color(), warning);
            ui.add_space(4.0);
        }

        self.render_markdown_card(ui, "Instructions", &assignment.instructions_md);

        if let Some(printable) = self.assignment_printable_display(assignment) {
            ui.add_space(6.0);
            self.render_markdown_card(ui, "Worksheet / handout", &printable);
        }

        if !assignment.attachments.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Attached resources").strong());
            for attachment in &assignment.attachments {
                ui.label(format!("Attachment: {attachment}"));
                if let Some(preview) = self.load_text_attachment_preview(attachment, 16, 1800) {
                    egui::Frame::none()
                        .fill(color_from_hex(&self.theme.surface))
                        .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::vec2(10.0, 8.0))
                        .show(ui, |ui| {
                            render_markdown(ui, &preview);
                        });
                } else if self
                    .resolve_assignment_attachment_path(attachment)
                    .is_none()
                {
                    ui.colored_label(
                        self.warning_color(),
                        "Attachment file was not found in the current data/resources folders.",
                    );
                } else {
                    ui.label("Preview unavailable for this attachment type.");
                }
                ui.add_space(4.0);
            }
        }
    }

    fn build_assignment_context(&self, assignment: &HomeworkAssignment) -> String {
        let mut out = format!(
            "Assignment: {} - {}\nSubject: {}\nYear: {}\nDue: {}\nInstructions:\n{}\n",
            assignment.id,
            assignment.title,
            assignment.subject,
            assignment.year_level,
            assignment
                .due_at
                .clone()
                .unwrap_or_else(|| "not set".to_string()),
            assignment.instructions_md.trim()
        );

        if let Some(printable) = self.assignment_printable_display(assignment) {
            out.push_str("\nStudent worksheet / handout:\n");
            out.push_str(printable.trim());
            out.push('\n');
        }

        if !assignment.attachments.is_empty() {
            out.push_str("\nAttached resources:\n");
            for attachment in &assignment.attachments {
                out.push_str(&format!("- {attachment}\n"));
                if let Some(preview) = self.load_text_attachment_preview(attachment, 12, 900) {
                    out.push_str(preview.trim());
                    out.push('\n');
                }
            }
        }

        if let Some(warning) = self.assignment_resource_warning(assignment) {
            out.push_str("\nResource note:\n");
            out.push_str(&warning);
            out.push('\n');
        }

        out
    }

    fn build_memory_context_block(&self) -> String {
        let memory_jogger = self.memory_jogger_items();
        let mut sections = Vec::new();

        if let Some(thoughts) = self.chatty_thoughts_prompt_block() {
            sections.push(format!("Chatty's thoughts for this session:\n{thoughts}"));
        }

        if !memory_jogger.is_empty() {
            sections.push(format!(
                "Memory jogger loaded at session start:\n- {}",
                memory_jogger.join("\n- ")
            ));
        }

        if sections.is_empty() {
            String::new()
        } else {
            format!("{}\n\n", sections.join("\n\n"))
        }
    }

    fn bookkeeper_chat_note(assignment: Option<&HomeworkAssignment>) -> Option<String> {
        assignment.map(|assignment| {
            format!(
                "Homework context: {} ({}) | {} | Year {}",
                assignment.title, assignment.id, assignment.subject, assignment.year_level
            )
        })
    }

    fn extract_question_number(student_question: &str) -> Option<usize> {
        let lower = student_question.to_ascii_lowercase();
        let tokens: Vec<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .collect();

        for window in tokens.windows(2) {
            if matches!(window[0], "question" | "q" | "number") {
                if let Ok(number) = window[1].parse::<usize>() {
                    return Some(number);
                }
            }
        }

        for token in tokens {
            if let Some(rest) = token.strip_prefix('q') {
                if let Ok(number) = rest.parse::<usize>() {
                    return Some(number);
                }
            }
        }

        None
    }

    fn parse_numbered_instruction_line(line: &str) -> Option<(usize, &str)> {
        let trimmed = line.trim();
        let mut digits_end = 0usize;
        for ch in trimmed.chars() {
            if ch.is_ascii_digit() {
                digits_end += ch.len_utf8();
            } else {
                break;
            }
        }
        if digits_end == 0 {
            return None;
        }
        let number = trimmed[..digits_end].parse::<usize>().ok()?;
        let rest = trimmed[digits_end..].trim_start();
        if let Some(stripped) = rest.strip_prefix('.') {
            return Some((number, stripped.trim_start()));
        }
        if let Some(stripped) = rest.strip_prefix(')') {
            return Some((number, stripped.trim_start()));
        }
        None
    }

    fn referenced_instruction<'a>(
        &self,
        assignment: &'a HomeworkAssignment,
        student_question: &str,
    ) -> Option<(usize, &'a str)> {
        let wanted = Self::extract_question_number(student_question)?;
        for line in assignment.instructions_md.lines() {
            if let Some((number, rest)) = Self::parse_numbered_instruction_line(line) {
                if number == wanted {
                    return Some((number, rest));
                }
            }
        }
        None
    }

    fn score_homework_question_match(
        normalized_input: &str,
        input_tokens: &HashSet<String>,
        question: &HomeworkQuestionIntercept,
        mentioned_number: Option<usize>,
    ) -> f32 {
        if normalized_input.is_empty() || input_tokens.is_empty() {
            return 0.0;
        }

        if mentioned_number.is_some() && question.question_number == mentioned_number {
            return 1.0;
        }
        if question.normalized_question.len() >= 8
            && normalized_input.contains(&question.normalized_question)
        {
            return 0.98;
        }
        if normalized_input.len() >= 8 && question.normalized_question.contains(normalized_input) {
            return 0.9;
        }

        let signature_hit = question
            .signature_phrases
            .iter()
            .filter(|phrase| normalized_input.contains(phrase.as_str()))
            .count();
        let has_mathy_question = !question.number_tokens.is_empty()
            || question
                .question_text
                .chars()
                .any(|ch| matches!(ch, '+' | '-' | '*' | '/' | '=' | '^'));
        if signature_hit > 0 && has_mathy_question {
            return 0.95;
        }

        let keyword_hits = question
            .keyword_tokens
            .iter()
            .filter(|token| input_tokens.contains(*token))
            .count();
        let number_hits = question
            .number_tokens
            .iter()
            .filter(|token| {
                input_tokens.contains(*token) || normalized_input.contains(token.as_str())
            })
            .count();
        let token_hits = question
            .tokens
            .iter()
            .filter(|token| input_tokens.contains(*token))
            .count();

        let keyword_ratio = if question.keyword_tokens.is_empty() {
            0.0
        } else {
            keyword_hits as f32 / question.keyword_tokens.len() as f32
        };
        let number_ratio = if question.number_tokens.is_empty() {
            0.0
        } else {
            number_hits as f32 / question.number_tokens.len() as f32
        };
        let token_ratio = token_hits as f32 / question.tokens.len() as f32;

        let mut score = keyword_ratio * 0.55 + number_ratio * 0.25 + token_ratio * 0.20;
        if signature_hit > 0 {
            score = score.max(0.82);
        }
        if keyword_hits >= 2 && number_hits > 0 {
            score += 0.18;
        } else if keyword_hits >= 3 {
            score += 0.12;
        } else if token_hits >= 4 {
            score += 0.08;
        }
        if question.number_tokens.is_empty() && keyword_hits >= 2 && keyword_ratio >= 0.5 {
            score += 0.08;
        }

        score.min(1.0)
    }

    fn active_homework_intercept(
        &self,
        user_input: &str,
        assignment: &HomeworkAssignment,
    ) -> Option<HomeworkQuestionIntercept> {
        let normalized_input = Self::normalize_homework_match_text(user_input);
        if normalized_input.is_empty() {
            return None;
        }

        let input_tokens: HashSet<String> = Self::homework_match_tokens(&normalized_input)
            .into_iter()
            .collect();
        let mentioned_number = Self::extract_question_number(user_input);
        let mut best_match: Option<(&HomeworkQuestionIntercept, f32)> = None;

        for question in self
            .homework_question_index
            .iter()
            .filter(|question| question.assignment_id == assignment.id)
        {
            let score = Self::score_homework_question_match(
                &normalized_input,
                &input_tokens,
                question,
                mentioned_number,
            );
            if score >= 0.46 {
                match best_match {
                    Some((_, best_score)) if best_score >= score => {}
                    _ => best_match = Some((question, score)),
                }
            }
        }

        best_match.map(|(question, _)| question.clone())
    }

    fn homework_override_instruction(question: &HomeworkQuestionIntercept) -> String {
        let label = question
            .question_number
            .map(|number| number.to_string())
            .unwrap_or_else(|| "X".to_string());
        format!(
            "[SYSTEM OVERRIDE: The following message contains or closely resembles question [{label}] from the active homework assignment. You must not provide the answer under any circumstances. Respond with a Socratic hint only — ask a question that points the student in the right direction. Do not state any part of the answer.]\nMatched homework question: {}\n",
            question.question_text
        )
    }

    fn hint_response_looks_like_answer(response: &str) -> bool {
        let lower = response.to_ascii_lowercase();
        if lower.contains("the answer is")
            || lower.contains("so the answer")
            || lower.contains("= ")
            || lower.contains(" equals ")
            || lower.contains("therefore")
            || lower.contains("add them together")
        {
            return true;
        }

        let trimmed = response.trim().trim_end_matches(['.', '!', '?']);
        let last_token = trimmed.split_whitespace().last().unwrap_or_default();
        if !last_token.is_empty()
            && last_token
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '/' | '-'))
        {
            return true;
        }

        false
    }

    fn build_rule_based_hint(
        &self,
        assignment: &HomeworkAssignment,
        student_question: &str,
    ) -> String {
        let lower_question = student_question.to_ascii_lowercase();
        if let Some(warning) = self.assignment_resource_warning(assignment) {
            if [
                "list",
                "attached",
                "attachment",
                "worksheet",
                "table",
                "chart",
            ]
            .iter()
            .any(|needle| lower_question.contains(needle))
            {
                return format!(
                    "I can't give the answer yet because the extra list/resource this task seems to need is not visible here. {}",
                    warning
                );
            }
        }

        if let Some((number, line)) = self.referenced_instruction(assignment, student_question) {
            let lowered = line.to_ascii_lowercase();
            if lowered.contains("list")
                || lowered.contains("attached")
                || lowered.contains("worksheet")
            {
                if let Some(warning) = self.assignment_resource_warning(assignment) {
                    return format!(
                        "I can't give the answer yet because question {} seems to rely on an extra worksheet/list/resource that is not visible here. {}",
                        number, warning
                    );
                }
            }
            let hint = if lowered.contains("sum")
                || lowered.contains("add up")
                || lowered.contains("total")
            {
                "Start by listing exactly the values you need, then add them one step at a time and check that you used each value once."
            } else if lowered.contains("solve for x")
                || lowered.contains("solve for y")
                || lowered.contains("equation")
            {
                "Try isolating the variable by undoing the operations around it in reverse order."
            } else if lowered.contains("area") && lowered.contains("triangle") {
                "Write the triangle area rule first, substitute the base and height, and remember it is half of the matching rectangle area."
            } else if lowered.contains("area") && lowered.contains("rectangle") {
                "Write the rectangle area rule first, then substitute the given length and width before multiplying."
            } else if lowered.contains("perimeter") {
                "Perimeter means the total distance around the outside, so think about which side lengths need to be added."
            } else if lowered.contains("decimal") && lowered.contains('/') {
                "Turn the fraction into a division problem: numerator divided by denominator."
            } else if lowered.contains("volume") && lowered.contains("cube") {
                "A cube has the same side length in all three dimensions, so think about multiplying the side length three times."
            } else if lowered.contains("square root") {
                "Ask yourself which number multiplied by itself gives the target number."
            } else {
                "Write down what the question is asking you to find, underline the useful numbers or facts, and do only the first step."
            };

            return format!(
                "I can't give the answer, but here's a hint for question {}: {}",
                number, hint
            );
        }

        if let Some(warning) = self.assignment_resource_warning(assignment) {
            return format!(
                "I can't give the answer, but I can point out the issue first: {}",
                warning
            );
        }

        "I can't give the answer, but here's a way to think about it: write down what the question is asking you to find, list the information it gives you, and do only the first step.".to_string()
    }

    fn safe_homework_hint_response(
        &self,
        assignment: &HomeworkAssignment,
        student_question: &str,
        model_response: &str,
    ) -> String {
        let cleaned = Self::normalize_model_message(model_response);
        if cleaned.trim().is_empty() || Self::hint_response_looks_like_answer(&cleaned) {
            self.build_rule_based_hint(assignment, student_question)
        } else {
            cleaned
        }
    }

    fn is_homework_related_message(
        &self,
        user_input: &str,
        assignment: &HomeworkAssignment,
    ) -> bool {
        let lower = user_input.to_ascii_lowercase();
        if [
            "homework",
            "assignment",
            "question",
            "worksheet",
            "hint",
            "help",
            "answer",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
        {
            return true;
        }

        lower.contains(&assignment.id.to_ascii_lowercase())
            || lower.contains(&assignment.title.to_ascii_lowercase())
            || lower.contains(&assignment.subject.to_ascii_lowercase())
    }

    fn render_submission_area(&mut self, ui: &mut egui::Ui) {
        ui.label("Type your work and export a submission file to upload via the portal.");
        ui.add(
            egui::TextEdit::multiline(&mut self.submission_text)
                .hint_text("Your answers, notes, or summary..."),
        );
        ui.horizontal(|ui| {
            if ui.button("Add attachments...").clicked() {
                if let Some(files) = FileDialog::new().pick_files() {
                    for f in files {
                        if let Some(p) = f.to_str() {
                            self.submission_attachments.push(p.to_string());
                        }
                    }
                }
            }
            if ui.button("Clear attachments").clicked() {
                self.submission_attachments.clear();
            }
        });
        if !self.submission_attachments.is_empty() {
            ui.label("Attachments:");
            let mut to_remove: Option<usize> = None;
            for (idx, path) in self.submission_attachments.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{path}"));
                    if ui.small_button("x").clicked() {
                        to_remove = Some(idx);
                    }
                });
            }
            if let Some(idx) = to_remove {
                self.submission_attachments.remove(idx);
            }
        }
        let assignment = self.selected_assignment_ref().cloned();
        let disabled = assignment.is_none();
        if ui
            .add_enabled(!disabled, egui::Button::new("Export submission file"))
            .clicked()
        {
            if let Some(assignment) = assignment {
                self.pulse_ecg(52.0, "Writing a student submission file.");
                match save_submission_for_assignment(
                    &self.base_path,
                    &self.settings,
                    &assignment,
                    &self.submission_text,
                    &self.submission_attachments,
                ) {
                    Ok(path) => {
                        let _ = ui.label(format!("Saved to {}", path.display()));
                        self.submission_text.clear();
                        self.submission_attachments.clear();
                        self.resync_homework();
                        self.resync_revision();
                    }
                    Err(e) => {
                        let _ = ui.label(format!("Failed: {e}"));
                    }
                }
            }
        }
    }

    fn submission_rows(&self) -> Vec<SubmissionRow> {
        let mut rows = Vec::new();
        for s in &self.submissions {
            let (title, subject) = self
                .current_pack
                .as_ref()
                .and_then(|p| {
                    p.assignments
                        .iter()
                        .find(|a| a.id == s.assignment_id)
                        .map(|a| (a.title.clone(), a.subject.clone()))
                })
                .unwrap_or_else(|| ("Assignment".to_string(), "General".to_string()));
            let score = s
                .ai_score
                .or(s.score)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let feedback = s
                .ai_feedback
                .clone()
                .unwrap_or_else(|| "No AI feedback".to_string());
            rows.push(SubmissionRow {
                assignment_id: s.assignment_id.clone(),
                assignment_title: title,
                student_id: s.student_id.clone(),
                student_name: s.student_name.clone(),
                subject,
                score,
                feedback,
                submitted_at: s.submitted_at.clone(),
            });
        }
        rows
    }

    fn score_entries(&self) -> Vec<StudentScore> {
        self.submissions
            .iter()
            .map(|s| {
                let subject = self
                    .current_pack
                    .as_ref()
                    .and_then(|p| {
                        p.assignments
                            .iter()
                            .find(|a| a.id == s.assignment_id)
                            .map(|a| a.subject.clone())
                    })
                    .unwrap_or_else(|| "General".to_string());
                let score_val = s.ai_score.or(s.score).unwrap_or(0) as f32;
                StudentScore {
                    student_id: s.student_id.clone(),
                    student_name: s.student_name.clone(),
                    subject,
                    score: score_val,
                }
            })
            .collect()
    }

    fn build_chat_system_prompt(
        &self,
        assignment: Option<&HomeworkAssignment>,
        intercept: Option<&HomeworkQuestionIntercept>,
    ) -> String {
        let memory_context = self.build_memory_context_block();
        if let Some(assignment) = assignment {
            let homework_rules = if self.settings.homework_hints_only {
                "Homework-aware mode: if the user's message is about the assignment below, use that context and answer with hints only. Never provide a full solution, final answer, or text that could be submitted. Give a short hint, a few steps, or a guiding question instead."
            } else {
                "Homework-aware mode: if the user's message is about the assignment below, use that context. Prefer helpful tutoring, explanation, and guidance over just giving the final answer. If the request is unrelated to homework, ignore the assignment context and answer normally."
            };
            let override_text = intercept
                .map(Self::homework_override_instruction)
                .unwrap_or_default();

            return format!(
                "{capsule}\n{memory_context}{rules}\n{override_text}Current assignment context:\n{context}\nRespond with one short, clear answer. Only use the assignment context if the student's next message appears to be about that homework.",
                capsule = CHAT_CAPSULE,
                memory_context = memory_context,
                rules = homework_rules,
                override_text = override_text,
                context = self.build_assignment_context(assignment),
            );
        }

        format!(
            "{capsule}\n{memory_context}Respond with one short, clear answer.",
            capsule = CHAT_CAPSULE,
            memory_context = memory_context
        )
    }

    fn handle_chat_send(&mut self) {
        if self.chat_input.trim().is_empty() {
            return;
        }
        let user_msg = self.chat_input.trim().to_string();
        let selected_assignment = self.selected_assignment_ref().cloned();
        let homework_intercept = selected_assignment
            .as_ref()
            .and_then(|assignment| self.active_homework_intercept(&user_msg, assignment));
        let enforce_homework_hints = self.settings.homework_hints_only;
        let system_prompt = self
            .build_chat_system_prompt(selected_assignment.as_ref(), homework_intercept.as_ref());
        let bookkeeper_note = Self::bookkeeper_chat_note(selected_assignment.as_ref());
        self.pulse_ecg(88.0, "Generating a chat response with the local model.");
        self.chat_log.push(("You".to_string(), user_msg.clone()));
        if let Some(bookkeeper) = self.bookkeeper.as_ref() {
            bookkeeper.append_chat_entry("You", &user_msg, bookkeeper_note.clone());
        }
        // Show a placeholder before generation to avoid disappearing messages
        self.chat_log
            .push(("Chatty".to_string(), "...".to_string()));

        let result = panic::catch_unwind({
            let settings = self.settings.clone();
            let user_msg = user_msg.clone();
            move || generate_answer_with_system_prompt(&settings, &system_prompt, &user_msg)
        });

        let chat_response = match result {
            Ok(filtered) => {
                if let Some(assignment) = selected_assignment.as_ref() {
                    if homework_intercept.is_some()
                        || (enforce_homework_hints
                            && self.is_homework_related_message(&user_msg, assignment))
                    {
                        self.safe_homework_hint_response(assignment, &user_msg, &filtered)
                    } else {
                        Self::normalize_model_message(&filtered)
                    }
                } else {
                    Self::normalize_model_message(&filtered)
                }
            }
            Err(_) => "Sorry, I ran into an error while answering.".to_string(),
        };

        if let Some(last) = self.chat_log.last_mut() {
            last.1 = chat_response.clone();
        }
        if let Some(bookkeeper) = self.bookkeeper.as_ref() {
            bookkeeper.append_chat_entry("Chatty", &chat_response, bookkeeper_note);
        }
        self.chat_input.clear();
    }
}
fn render_markdown(ui: &mut egui::Ui, text: &str) {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            ui.heading(trimmed.trim_start_matches("# ").trim());
        } else if trimmed.starts_with("## ") {
            ui.label(RichText::new(trimmed.trim_start_matches("## ").trim()).strong());
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            ui.label(format!("* {}", trimmed[2..].trim()));
        } else if trimmed.is_empty() {
            ui.add_space(6.0);
        } else {
            ui.label(trimmed);
        }
    }
}

fn aggregate_scores(entries: &[StudentScore]) -> (f32, Vec<(String, f32)>, Vec<(String, f32)>) {
    let mut per_student: HashMap<String, Vec<f32>> = HashMap::new();
    let mut per_subject: HashMap<String, Vec<f32>> = HashMap::new();
    for e in entries {
        per_student
            .entry(e.student_name.clone())
            .or_default()
            .push(e.score);
        per_subject
            .entry(e.subject.clone())
            .or_default()
            .push(e.score);
    }

    let avg = |vals: &[f32]| -> f32 {
        if vals.is_empty() {
            0.0
        } else {
            vals.iter().copied().sum::<f32>() / vals.len() as f32
        }
    };

    let class_overall = avg(&entries.iter().map(|e| e.score).collect::<Vec<_>>());

    let mut per_student_avg: Vec<(String, f32)> =
        per_student.into_iter().map(|(k, v)| (k, avg(&v))).collect();
    per_student_avg.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut per_subject_avg: Vec<(String, f32)> =
        per_subject.into_iter().map(|(k, v)| (k, avg(&v))).collect();
    per_subject_avg.sort_by(|a, b| a.0.cmp(&b.0));

    (class_overall, per_student_avg, per_subject_avg)
}

fn score_color(score: f32) -> egui::Color32 {
    let t = (score / 100.0).clamp(0.0, 1.0);
    let r = ((1.0 - t) * 255.0) as u8;
    let g = (t * 200.0 + 55.0).min(255.0) as u8;
    egui::Color32::from_rgb(r, g, 64)
}

fn color_from_hex(hex: &str) -> egui::Color32 {
    let h = hex.trim_start_matches('#');
    if h.len() == 6 {
        if let Ok(rgb) = u32::from_str_radix(h, 16) {
            let r = ((rgb >> 16) & 0xFF) as u8;
            let g = ((rgb >> 8) & 0xFF) as u8;
            let b = (rgb & 0xFF) as u8;
            return egui::Color32::from_rgb(r, g, b);
        }
    } else if h.len() == 8 {
        if let Ok(rgba) = u32::from_str_radix(h, 16) {
            let r = ((rgba >> 24) & 0xFF) as u8;
            let g = ((rgba >> 16) & 0xFF) as u8;
            let b = ((rgba >> 8) & 0xFF) as u8;
            let a = (rgba & 0xFF) as u8;
            return egui::Color32::from_rgba_premultiplied(r, g, b, a);
        }
    }
    egui::Color32::GRAY
}

#[derive(Debug, Clone, Default)]
struct GgufMetadata {
    version: u32,
    tensor_count: u64,
    kv_count: u64,
    general_architecture: Option<String>,
    general_name: Option<String>,
    general_file_type: Option<i64>,
    general_quantization_version: Option<i64>,
    tokenizer_model: Option<String>,
    context_length: Option<u64>,
    tensor_ggml_type_max: Option<u32>,
    tensor_ggml_type_unique: Option<usize>,
    tensor_ggml_type_top: Vec<(u32, usize)>,
}

fn gguf_metadata_summary(path: &Path) -> Result<String, String> {
    let meta = read_gguf_metadata(path)?;
    let mut out = String::new();
    out.push_str(&format!("- gguf_version: {}\n", meta.version));
    out.push_str(&format!("- gguf_kv_count: {}\n", meta.kv_count));
    out.push_str(&format!("- gguf_tensor_count: {}\n", meta.tensor_count));
    if let Some(v) = meta.general_architecture.as_deref() {
        out.push_str(&format!("- gguf_architecture: {v}\n"));
    }
    if let Some(v) = meta.general_name.as_deref() {
        out.push_str(&format!("- gguf_general_name: {v}\n"));
    }
    if let Some(v) = meta.general_file_type {
        out.push_str(&format!("- gguf_file_type: {v}\n"));
    }
    if let Some(v) = meta.general_quantization_version {
        out.push_str(&format!("- gguf_quantization_version: {v}\n"));
    }
    if let Some(v) = meta.tokenizer_model.as_deref() {
        out.push_str(&format!("- gguf_tokenizer_model: {v}\n"));
    }
    if let Some(v) = meta.context_length {
        out.push_str(&format!("- gguf_context_length: {v}\n"));
    }
    if let Some(v) = meta.tensor_ggml_type_max {
        out.push_str(&format!("- gguf_tensor_ggml_type_max_id: {v}\n"));
    }
    if let Some(v) = meta.tensor_ggml_type_unique {
        out.push_str(&format!("- gguf_tensor_ggml_type_unique: {v}\n"));
    }
    if !meta.tensor_ggml_type_top.is_empty() {
        let list = meta
            .tensor_ggml_type_top
            .iter()
            .map(|(id, n)| format!("{id}={n}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("- gguf_tensor_ggml_type_top: {list}\n"));
    }
    Ok(out)
}

fn read_gguf_metadata(path: &Path) -> Result<GgufMetadata, String> {
    const MAX_GGUF_STRING_BYTES: u64 = 1_048_576; // 1 MiB
    const MAX_GGUF_ARRAY_LEN: u64 = 1_000_000;
    const MAX_KV_COUNT: u64 = 50_000;
    const MAX_TENSOR_COUNT: u64 = 5_000_000;
    const MAX_DIMS: u32 = 64;

    let mut f = fs::File::open(path)
        .map_err(|e| format!("Could not open GGUF file {}: {e}", path.display()))?;

    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)
        .map_err(|e| format!("Could not read GGUF header {}: {e}", path.display()))?;
    if &magic != b"GGUF" {
        return Err("Missing GGUF magic header".to_string());
    }

    let version = read_u32_le(&mut f)?;
    let tensor_count = read_u64_le(&mut f)?;
    let kv_count = read_u64_le(&mut f)?;

    if kv_count > MAX_KV_COUNT {
        return Err(format!(
            "GGUF kv_count too large to parse safely: {kv_count}"
        ));
    }
    if tensor_count > MAX_TENSOR_COUNT {
        return Err(format!(
            "GGUF tensor_count too large to parse safely: {tensor_count}"
        ));
    }

    let mut meta = GgufMetadata {
        version,
        tensor_count,
        kv_count,
        ..Default::default()
    };

    for _ in 0..kv_count {
        let key = read_gguf_string(&mut f, MAX_GGUF_STRING_BYTES)?;
        let value_type = read_u32_le(&mut f)?;
        match key.as_str() {
            "general.architecture" => {
                if value_type == 8 {
                    meta.general_architecture =
                        Some(read_gguf_string(&mut f, MAX_GGUF_STRING_BYTES)?);
                } else {
                    skip_gguf_value(
                        &mut f,
                        value_type,
                        MAX_GGUF_STRING_BYTES,
                        MAX_GGUF_ARRAY_LEN,
                    )?;
                }
            }
            "general.name" => {
                if value_type == 8 {
                    meta.general_name = Some(read_gguf_string(&mut f, MAX_GGUF_STRING_BYTES)?);
                } else {
                    skip_gguf_value(
                        &mut f,
                        value_type,
                        MAX_GGUF_STRING_BYTES,
                        MAX_GGUF_ARRAY_LEN,
                    )?;
                }
            }
            "general.file_type" => match value_type {
                4 => meta.general_file_type = Some(read_u32_le(&mut f)? as i64),
                5 => meta.general_file_type = Some(read_i32_le(&mut f)? as i64),
                10 => meta.general_file_type = Some(read_u64_le(&mut f)? as i64),
                11 => meta.general_file_type = Some(read_i64_le(&mut f)?),
                _ => skip_gguf_value(
                    &mut f,
                    value_type,
                    MAX_GGUF_STRING_BYTES,
                    MAX_GGUF_ARRAY_LEN,
                )?,
            },
            "general.quantization_version" => match value_type {
                4 => meta.general_quantization_version = Some(read_u32_le(&mut f)? as i64),
                5 => meta.general_quantization_version = Some(read_i32_le(&mut f)? as i64),
                10 => meta.general_quantization_version = Some(read_u64_le(&mut f)? as i64),
                11 => meta.general_quantization_version = Some(read_i64_le(&mut f)?),
                _ => skip_gguf_value(
                    &mut f,
                    value_type,
                    MAX_GGUF_STRING_BYTES,
                    MAX_GGUF_ARRAY_LEN,
                )?,
            },
            "tokenizer.ggml.model" => {
                if value_type == 8 {
                    meta.tokenizer_model = Some(read_gguf_string(&mut f, MAX_GGUF_STRING_BYTES)?);
                } else {
                    skip_gguf_value(
                        &mut f,
                        value_type,
                        MAX_GGUF_STRING_BYTES,
                        MAX_GGUF_ARRAY_LEN,
                    )?;
                }
            }
            k if k.ends_with(".context_length") => match value_type {
                4 => meta.context_length = Some(read_u32_le(&mut f)? as u64),
                5 => meta.context_length = Some(read_i32_le(&mut f)? as u64),
                10 => meta.context_length = Some(read_u64_le(&mut f)?),
                11 => meta.context_length = Some(read_i64_le(&mut f)? as u64),
                _ => skip_gguf_value(
                    &mut f,
                    value_type,
                    MAX_GGUF_STRING_BYTES,
                    MAX_GGUF_ARRAY_LEN,
                )?,
            },
            _ => {
                skip_gguf_value(
                    &mut f,
                    value_type,
                    MAX_GGUF_STRING_BYTES,
                    MAX_GGUF_ARRAY_LEN,
                )?;
            }
        }
    }

    let mut type_counts: HashMap<u32, usize> = HashMap::new();
    let mut max_type: u32 = 0;
    for _ in 0..tensor_count {
        skip_gguf_string(&mut f, MAX_GGUF_STRING_BYTES)?;
        let n_dims = read_u32_le(&mut f)?;
        if n_dims > MAX_DIMS {
            return Err(format!("GGUF tensor has too many dims ({n_dims})"));
        }
        for _ in 0..n_dims {
            let _ = read_u64_le(&mut f)?;
        }
        let t = read_u32_le(&mut f)?;
        let _ = read_u64_le(&mut f)?; // offset
        *type_counts.entry(t).or_insert(0) += 1;
        max_type = max_type.max(t);
    }

    meta.tensor_ggml_type_max = Some(max_type);
    meta.tensor_ggml_type_unique = Some(type_counts.len());

    let mut top: Vec<(u32, usize)> = type_counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top.truncate(10);
    meta.tensor_ggml_type_top = top;

    Ok(meta)
}

fn read_u32_le<R: Read>(mut r: R) -> Result<u32, String> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| format!("I/O error: {e}"))?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64_le<R: Read>(mut r: R) -> Result<u64, String> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)
        .map_err(|e| format!("I/O error: {e}"))?;
    Ok(u64::from_le_bytes(buf))
}

fn read_i32_le<R: Read>(mut r: R) -> Result<i32, String> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| format!("I/O error: {e}"))?;
    Ok(i32::from_le_bytes(buf))
}

fn read_i64_le<R: Read>(mut r: R) -> Result<i64, String> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)
        .map_err(|e| format!("I/O error: {e}"))?;
    Ok(i64::from_le_bytes(buf))
}

fn read_gguf_string<R: Read>(mut r: R, max_bytes: u64) -> Result<String, String> {
    let n = read_u64_le(&mut r)?;
    if n > max_bytes {
        return Err(format!("GGUF string too large: {n} bytes"));
    }
    let mut buf = vec![0u8; n as usize];
    r.read_exact(&mut buf)
        .map_err(|e| format!("I/O error: {e}"))?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn skip_gguf_string<R: Read + Seek>(mut r: R, max_bytes: u64) -> Result<(), String> {
    let n = read_u64_le(&mut r)?;
    if n > max_bytes {
        return Err(format!("GGUF string too large to skip: {n} bytes"));
    }
    skip_bytes(&mut r, n)
}

fn skip_bytes<R: Seek>(r: &mut R, n: u64) -> Result<(), String> {
    if n > i64::MAX as u64 {
        return Err("Seek too large".to_string());
    }
    r.seek(SeekFrom::Current(n as i64))
        .map(|_| ())
        .map_err(|e| format!("I/O seek error: {e}"))
}

fn skip_gguf_value<R: Read + Seek>(
    r: &mut R,
    t: u32,
    max_string_bytes: u64,
    max_array_len: u64,
) -> Result<(), String> {
    match t {
        0 | 1 | 7 => skip_bytes(r, 1),
        2 | 3 => skip_bytes(r, 2),
        4 | 5 | 6 => skip_bytes(r, 4),
        10 | 11 | 12 => skip_bytes(r, 8),
        8 => skip_gguf_string(r, max_string_bytes),
        9 => {
            let et = read_u32_le(&mut *r)?;
            let n = read_u64_le(&mut *r)?;
            if n > max_array_len {
                return Err(format!(
                    "GGUF array too large to skip safely: n={n}, et={et}"
                ));
            }
            for _ in 0..n {
                skip_gguf_value(r, et, max_string_bytes, max_array_len)?;
            }
            Ok(())
        }
        _ => Err(format!("Unknown GGUF value type id: {t}")),
    }
}

fn sanitize_filename_component(text: &str) -> String {
    text.trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl App for ChattyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        apply_theme(&self.theme, ctx);
        self.ecg_window.tick(Instant::now());
        ctx.request_repaint_after(self.ecg_window.refresh_interval());

        TopBottomPanel::top("menu_bar").show(ctx, |ui| self.render_menu_bar(ctx, ui));
        TopBottomPanel::top("tabs").show(ctx, |ui| self.render_tab_bar(ui));

        let show_homework_chat_bar = matches!(
            self.tabs.get(self.active_tab).map(|tab| &tab.kind),
            Some(TabKind::Home) | Some(TabKind::Chat)
        );
        if show_homework_chat_bar {
            TopBottomPanel::bottom("chat_input").show(ctx, |ui| {
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.label("Chat:");
                    let input = ui.add(
                        egui::TextEdit::singleline(&mut self.chat_input)
                            .hint_text("Ask or type a command..."),
                    );
                    if input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.handle_chat_send();
                    }
                    if ui.button("Send").clicked() {
                        self.handle_chat_send();
                    }
                });
            });
        }

        CentralPanel::default().show(ctx, |ui| {
            if let Some(tab) = self.tabs.get(self.active_tab).cloned() {
                match tab.kind {
                    TabKind::Home => self.render_home(ui),
                    TabKind::Chat => self.render_chat(ui),
                    TabKind::Bookkeeper => self.render_bookkeeper(ui),
                    TabKind::Settings => self.render_settings(ui),
                    TabKind::Diagnostics => self.render_diagnostics(ctx, ui),
                    TabKind::Module { .. } => self.render_module_tab(ui, self.active_tab),
                }
            }
        });
    }
}

impl Drop for ChattyApp {
    fn drop(&mut self) {
        if let Some(bookkeeper) = self.bookkeeper.take() {
            bookkeeper.shutdown_silently();
        }
    }
}

pub fn launch_gui(base_path: PathBuf, settings: Settings) -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Chatty-EDU")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Chatty-EDU",
        native_options,
        Box::new(move |cc| {
            let app =
                ChattyApp::new(cc, base_path.clone(), settings.clone()).unwrap_or_else(|_| {
                    ChattyApp::new(cc, base_path.clone(), settings.clone())
                        .expect("Failed to start app")
                });
            Box::new(app)
        }),
    )
}
