use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::settings::Settings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeworkAssignment {
    pub id: String,
    pub title: String,
    pub subject: String,
    pub year_level: String,
    pub due_at: Option<String>,
    pub instructions_md: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub student_printable_md: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teacher_rubric_md: Option<String>,
    #[serde(default)]
    pub attachments: Vec<String>,
    #[serde(default = "default_allow_games")]
    pub allow_games: bool,
    #[serde(default)]
    pub allow_ai_premark: bool,
    pub max_score: Option<i32>,
}

fn default_allow_games() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeworkPack {
    pub version: String,
    pub school_id: String,
    pub class_id: String,
    pub created_at: String,
    pub assignments: Vec<HomeworkAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerEntry {
    pub question: String,
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPremark {
    pub score: Option<i32>,
    pub feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeworkSubmission {
    pub version: String,
    pub school_id: String,
    pub class_id: String,
    pub assignment_id: String,
    #[serde(default)]
    pub assignment_title: Option<String>,
    #[serde(default)]
    pub assignment_subject: Option<String>,
    #[serde(default)]
    pub assignment_year_level: Option<String>,
    #[serde(default)]
    pub assignment_instructions_md: Option<String>,
    pub student_id: String,
    pub student_name: String,
    pub submitted_at: String,
    #[serde(default)]
    pub answers_text: Option<String>,
    #[serde(default)]
    pub answers: Vec<AnswerEntry>,
    #[serde(default)]
    pub ai_premark: Option<AiPremark>,
    #[serde(default)]
    pub attachments: Vec<String>,
    #[serde(default)]
    pub events: Vec<SubmissionEvent>,
    #[serde(default)]
    pub final_hash: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SubmissionSummary {
    pub assignment_id: String,
    pub student_name: String,
    pub student_id: String,
    #[allow(dead_code)]
    pub submitted_at: String,
    pub score: Option<i32>,
    pub ai_score: Option<i32>,
    pub ai_feedback: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LoadedSubmission {
    pub path: PathBuf,
    pub submission: HomeworkSubmission,
}

pub fn export_pack_template(base: &Path, school_id: &str, class_id: &str) -> io::Result<PathBuf> {
    let pack = HomeworkPack {
        version: "1.0".to_string(),
        school_id: school_id.to_string(),
        class_id: class_id.to_string(),
        created_at: iso_now(),
        assignments: vec![HomeworkAssignment {
            id: "hw-sample-001".to_string(),
            title: "Sample homework".to_string(),
            subject: "General".to_string(),
            year_level: "7".to_string(),
            due_at: None,
            instructions_md: "Add your instructions here.\n- Question 1\n- Question 2".to_string(),
            student_printable_md: None,
            teacher_rubric_md: None,
            attachments: vec![],
            allow_games: false,
            allow_ai_premark: true,
            max_score: Some(100),
        }],
    };

    let dir = base.join("homework").join("assigned");
    fs::create_dir_all(&dir)?;
    let path = dir.join("homework_pack_template.json");
    let json = serde_json::to_string_pretty(&pack)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn create_pack(
    base: &Path,
    school_id: &str,
    class_id: &str,
    assignment: HomeworkAssignment,
) -> io::Result<PathBuf> {
    create_pack_multi(base, school_id, class_id, vec![assignment])
}

pub fn create_pack_multi(
    base: &Path,
    school_id: &str,
    class_id: &str,
    assignments: Vec<HomeworkAssignment>,
) -> io::Result<PathBuf> {
    let pack = HomeworkPack {
        version: "1.0".to_string(),
        school_id: school_id.to_string(),
        class_id: class_id.to_string(),
        created_at: iso_now(),
        assignments,
    };

    let dir = base.join("homework").join("assigned");
    fs::create_dir_all(&dir)?;
    let filename = format!(
        "homework_pack_{}_{}.json",
        class_id,
        pack.created_at.replace(':', "-")
    );
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(&pack)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load_pack_from_file(path: &Path) -> io::Result<HomeworkPack> {
    let contents = fs::read_to_string(path)?;
    let pack: HomeworkPack = serde_json::from_str(&contents)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("pack parse error: {e}")))?;
    Ok(normalize_pack(pack))
}

pub fn find_latest_pack(base: &Path) -> io::Result<Option<(PathBuf, HomeworkPack)>> {
    let dir = base.join("homework").join("assigned");
    // Always try to sync packs from the repo folder into the runtime data dir
    // so teacher/student dashboards see the latest files when running from source.
    if let Err(e) = sync_homework_packs_from_repo(base) {
        eprintln!("[homework] Could not sync sample packs: {e}");
    }
    if !dir.exists() {
        return Ok(None);
    }

    let mut newest: Option<(PathBuf, HomeworkPack, i128)> = None;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !is_pack_file(&path) {
            continue;
        }

        let contents = fs::read_to_string(&path)?;
        let pack: HomeworkPack = serde_json::from_str(&contents)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("pack parse error: {e}")))?;
        let pack = normalize_pack(pack);
        let ts = pack_timestamp(&pack, &entry.metadata().ok());

        match &newest {
            Some((_, _, current_ts)) if *current_ts >= ts => {}
            _ => newest = Some((path, pack, ts)),
        }
    }

    Ok(newest.map(|(path, pack, _)| (path, pack)))
}

pub fn apply_pack_policy(settings: &mut Settings, pack: &HomeworkPack) {
    let any_games_disallowed = pack.assignments.iter().any(|a| !a.allow_games);
    if any_games_disallowed {
        settings.game.enabled = false;
        settings.game.games_in_class_allowed = false;
    }
}

pub fn save_submission_with_answers(
    base: &Path,
    settings: &Settings,
    assignment_id: &str,
    answers_text: &str,
    attachments: &[String],
) -> io::Result<PathBuf> {
    save_submission_with_snapshot(
        base,
        settings,
        assignment_id,
        None,
        answers_text,
        attachments,
    )
}

pub fn save_submission_for_assignment(
    base: &Path,
    settings: &Settings,
    assignment: &HomeworkAssignment,
    answers_text: &str,
    attachments: &[String],
) -> io::Result<PathBuf> {
    save_submission_with_snapshot(
        base,
        settings,
        &assignment.id,
        Some(assignment),
        answers_text,
        attachments,
    )
}

fn save_submission_with_snapshot(
    base: &Path,
    settings: &Settings,
    assignment_id: &str,
    assignment: Option<&HomeworkAssignment>,
    answers_text: &str,
    attachments: &[String],
) -> io::Result<PathBuf> {
    let dir = base.join("homework").join("completed");
    fs::create_dir_all(&dir)?;

    let student_id = if settings.student.student_id.is_empty() {
        "student-id".to_string()
    } else {
        settings.student.student_id.clone()
    };
    let student_name = if settings.student.student_name.is_empty() {
        "Student".to_string()
    } else {
        settings.student.student_name.clone()
    };
    let class_id = if settings.student.class_id.is_empty() {
        "class".to_string()
    } else {
        settings.student.class_id.clone()
    };

    let premark = simple_premark(answers_text);
    let now_ms = unix_ms_now();
    let mut events = Vec::new();
    let start_event = build_event("", now_ms, "start", None, Some("session_start"));
    let answer_event = build_event(
        &start_event.hash,
        unix_ms_now(),
        "answer",
        Some("freeform"),
        Some(answers_text),
    );
    let finalize_event = build_event(
        &answer_event.hash,
        unix_ms_now(),
        "finalize",
        None,
        Some("submitted"),
    );
    events.push(start_event);
    events.push(answer_event);
    events.push(finalize_event.clone());

    let submission = HomeworkSubmission {
        version: "1.0".to_string(),
        school_id: "school".to_string(),
        class_id,
        assignment_id: assignment_id.to_string(),
        assignment_title: assignment.map(|assignment| assignment.title.clone()),
        assignment_subject: assignment.map(|assignment| assignment.subject.clone()),
        assignment_year_level: assignment.map(|assignment| assignment.year_level.clone()),
        assignment_instructions_md: assignment.map(|assignment| assignment.instructions_md.clone()),
        student_id: student_id.clone(),
        student_name: student_name.clone(),
        submitted_at: iso_now(),
        answers_text: Some(answers_text.to_string()),
        answers: vec![],
        ai_premark: Some(premark),
        attachments: attachments.to_vec(),
        events,
        final_hash: Some(finalize_event.hash),
        summary: None,
    };

    let filename = format!("submission_{}_{}.json", assignment_id, student_id);
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(&submission)?;
    fs::write(&path, json)?;
    Ok(path)
}

impl HomeworkSubmission {
    pub fn score_field(&self) -> Option<i32> {
        self.ai_premark.as_ref().and_then(|p| p.score)
    }
}

pub fn load_submission_summaries(base: &Path) -> io::Result<Vec<SubmissionSummary>> {
    let submissions = load_completed_submissions(base)?;
    let mut out = Vec::new();

    for record in submissions {
        let sub = record.submission;
        let ai_score = sub.ai_premark.as_ref().and_then(|p| p.score);
        let ai_feedback = sub.ai_premark.as_ref().and_then(|p| p.feedback.clone());
        out.push(SubmissionSummary {
            assignment_id: sub.assignment_id.clone(),
            student_name: sub.student_name.clone(),
            student_id: sub.student_id.clone(),
            submitted_at: sub.submitted_at.clone(),
            score: sub.score_field(),
            ai_score,
            ai_feedback,
        });
    }

    Ok(out)
}

pub fn load_completed_submissions(base: &Path) -> io::Result<Vec<LoadedSubmission>> {
    let dir = base.join("homework").join("completed");
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().map(|e| e != "json").unwrap_or(true) {
            continue;
        }
        let contents = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok(sub) = serde_json::from_str::<HomeworkSubmission>(&contents) {
            out.push(LoadedSubmission {
                path: path.clone(),
                submission: sub,
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionEvent {
    pub t: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    pub prev: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventData {
    t: i64,
    #[serde(rename = "type")]
    event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    qid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<String>,
    prev: String,
}

fn build_event(
    prev: &str,
    t: i64,
    event_type: &str,
    qid: Option<&str>,
    payload: Option<&str>,
) -> SubmissionEvent {
    let data = EventData {
        t,
        event_type: event_type.to_string(),
        qid: qid.map(|s| s.to_string()),
        payload: payload.map(|s| s.to_string()),
        prev: prev.to_string(),
    };
    let canonical = serde_json::to_string(&data).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(prev.as_bytes());
    hasher.update(canonical.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    SubmissionEvent {
        t,
        event_type: event_type.to_string(),
        qid: qid.map(|s| s.to_string()),
        payload: payload.map(|s| s.to_string()),
        prev: prev.to_string(),
        hash,
    }
}

fn unix_ms_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn is_pack_file(path: &Path) -> bool {
    path.is_file()
        && path.extension().map(|e| e == "json").unwrap_or(false)
        && path
            .file_name()
            .map(|n| n.to_string_lossy().contains("homework_pack"))
            .unwrap_or(false)
}

/// Seed the runtime homework/assigned folder with any packs that ship in the repo (homework/assigned)
/// so dashboards have data out of the box when running from source. If files already exist in the
/// runtime dir, newer copies from the repo will replace them.
fn sync_homework_packs_from_repo(base: &Path) -> io::Result<()> {
    let target = base.join("homework").join("assigned");
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let fallback = cwd.join("homework").join("assigned");

    if target == fallback {
        return Ok(()); // nothing to seed if base is already the repo root
    }
    if !fallback.exists() {
        return Ok(());
    }

    fs::create_dir_all(&target)?;

    let mut copied = 0usize;
    for entry in fs::read_dir(&fallback)? {
        let entry = entry?;
        let src = entry.path();
        if !is_pack_file(&src) {
            continue;
        }
        let dest = target.join(entry.file_name());
        let should_copy = if dest.exists() {
            let src_time = fs::metadata(&src).and_then(|m| m.modified()).ok();
            let dest_time = fs::metadata(&dest).and_then(|m| m.modified()).ok();
            match (src_time, dest_time) {
                (Some(s), Some(d)) => s > d,
                (Some(_), None) => true,
                _ => false,
            }
        } else {
            true
        };

        if should_copy {
            fs::copy(&src, &dest)?;
            copied += 1;
        }
    }

    if copied > 0 {
        eprintln!(
            "[homework] Copied {copied} sample pack(s) into {}",
            target.display()
        );
    }
    Ok(())
}

fn iso_now() -> String {
    let now = chrono::Utc::now();
    now.to_rfc3339()
}

fn pack_timestamp(pack: &HomeworkPack, meta: &Option<std::fs::Metadata>) -> i128 {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&pack.created_at) {
        return dt.timestamp_millis() as i128;
    }
    meta.as_ref()
        .and_then(|m| m.modified().ok())
        .map(|t| system_time_millis(t))
        .unwrap_or(0)
}

fn system_time_millis(t: SystemTime) -> i128 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i128)
        .unwrap_or(0)
}

fn simple_premark(text: &str) -> AiPremark {
    let len = text.trim().len();
    let score = if len > 400 {
        90
    } else if len > 200 {
        80
    } else if len > 100 {
        70
    } else if len > 40 {
        60
    } else {
        50
    };
    let feedback = if len < 50 {
        "Try adding more detail to your answers.".to_string()
    } else if len < 150 {
        "Good start—check if all parts are addressed.".to_string()
    } else {
        "Looks thorough. Review for accuracy and clarity.".to_string()
    };
    AiPremark {
        score: Some(score),
        feedback: Some(feedback),
    }
}

fn normalize_pack(mut pack: HomeworkPack) -> HomeworkPack {
    let mut merged: Vec<HomeworkAssignment> = Vec::new();

    for assignment in pack.assignments.drain(..) {
        let assignment = normalize_assignment(assignment);
        if let Some(existing) = merged.iter_mut().find(|current| {
            current.id == assignment.id
                && current.title == assignment.title
                && current.subject == assignment.subject
                && current.year_level == assignment.year_level
        }) {
            merge_assignment(existing, assignment);
        } else {
            merged.push(assignment);
        }
    }

    pack.assignments = merged;
    pack
}

fn normalize_assignment(mut assignment: HomeworkAssignment) -> HomeworkAssignment {
    assignment.student_printable_md = assignment
        .student_printable_md
        .take()
        .and_then(|text| clean_optional_markdown_section(&text));
    assignment.teacher_rubric_md = assignment
        .teacher_rubric_md
        .take()
        .and_then(|text| clean_optional_markdown_section(&text));
    assignment.attachments = dedupe_strings(assignment.attachments);
    assignment
}

fn merge_assignment(existing: &mut HomeworkAssignment, incoming: HomeworkAssignment) {
    if existing.instructions_md.trim().is_empty() && !incoming.instructions_md.trim().is_empty() {
        existing.instructions_md = incoming.instructions_md.clone();
    }
    if existing.student_printable_md.is_none() && incoming.student_printable_md.is_some() {
        existing.student_printable_md = incoming.student_printable_md.clone();
    }
    if existing.teacher_rubric_md.is_none() && incoming.teacher_rubric_md.is_some() {
        existing.teacher_rubric_md = incoming.teacher_rubric_md.clone();
    }
    if existing.due_at.is_none() && incoming.due_at.is_some() {
        existing.due_at = incoming.due_at.clone();
    }
    if existing.max_score.is_none() && incoming.max_score.is_some() {
        existing.max_score = incoming.max_score;
    }
    existing.allow_games = existing.allow_games && incoming.allow_games;
    existing.allow_ai_premark = existing.allow_ai_premark || incoming.allow_ai_premark;

    let mut combined = existing.attachments.clone();
    combined.extend(incoming.attachments);
    existing.attachments = dedupe_strings(combined);
}

fn clean_optional_markdown_section(text: &str) -> Option<String> {
    let cleaned = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn dedupe_strings(items: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !out.iter().any(|existing: &String| existing == trimmed) {
            out.push(trimmed.to_string());
        }
    }
    out
}
