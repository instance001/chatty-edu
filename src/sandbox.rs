use anyhow::{anyhow, Context};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const SANDBOX_DIR_NAME: &str = "Chatty_Sandbox";
pub const DEFAULT_SANDBOX_SCRATCHPAD_REL_PATH: &str = "scratchpad/current.md";
pub const DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH: &str = "scratchpad/task_ledger.md";

#[derive(Debug, Clone)]
pub enum SandboxAction {
    Write {
        path: String,
        contents: String,
    },
    Append {
        path: String,
        contents: String,
    },
    Read {
        path: String,
    },
    List,
    Preload {
        paths: Vec<String>,
        include_list: bool,
        include_scratchpad: bool,
        include_ledger: bool,
        note: String,
    },
    Ledger {
        status: String,
        current_task: String,
        next_step: String,
        open_questions: Vec<String>,
        files_touched: Vec<String>,
        notes: Vec<String>,
    },
}

#[derive(Default, Debug, Clone)]
pub struct TaskLedgerSummary {
    pub status: String,
    pub current_task: String,
    pub next_step: String,
    pub open_questions: Vec<String>,
    pub files_touched: Vec<String>,
    pub notes: Vec<String>,
}

pub struct SandboxPreloadResult {
    pub prompt_block: String,
    pub loaded_count: usize,
}

pub fn sandbox_dir(base: &Path) -> PathBuf {
    base.join(SANDBOX_DIR_NAME)
}

pub fn ensure_sandbox_dir(base: &Path) -> anyhow::Result<PathBuf> {
    let dir = sandbox_dir(base);
    fs::create_dir_all(&dir).with_context(|| format!("mkdir {}", dir.display()))?;
    Ok(dir)
}

fn canonicalize_dir(dir: &Path) -> anyhow::Result<PathBuf> {
    if !dir.exists() {
        fs::create_dir_all(dir).with_context(|| format!("mkdir {}", dir.display()))?;
    }
    dir.canonicalize()
        .with_context(|| format!("canonicalize {}", dir.display()))
}

fn parse_rel_path(path: &str) -> anyhow::Result<PathBuf> {
    let trimmed = path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err(anyhow!("empty path"));
    }
    let candidate = PathBuf::from(&trimmed);
    if candidate.is_absolute() {
        return Err(anyhow!("absolute paths are not allowed"));
    }
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(anyhow!("path traversal is not allowed"));
    }
    Ok(candidate)
}

pub fn ensure_path_within_dir(dir: &Path, path: &Path) -> anyhow::Result<PathBuf> {
    let base = canonicalize_dir(dir)?;
    let canonical = if path.exists() {
        path.canonicalize()
            .with_context(|| format!("canonicalize {}", path.display()))?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow!("missing parent for {}", path.display()))?;
        fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
        let canonical_parent = parent
            .canonicalize()
            .with_context(|| format!("canonicalize {}", parent.display()))?;
        canonical_parent.join(
            path.file_name()
                .ok_or_else(|| anyhow!("missing file name for {}", path.display()))?,
        )
    };
    if canonical.starts_with(&base) {
        Ok(canonical)
    } else {
        Err(anyhow!("path escapes sandbox"))
    }
}

pub fn ensure_save_path_within_dir(dir: &Path, path: &Path) -> anyhow::Result<PathBuf> {
    ensure_path_within_dir(dir, path)
}

pub fn read_text_file(path: &Path, max_bytes: usize) -> anyhow::Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let clipped = if bytes.len() > max_bytes {
        &bytes[..max_bytes]
    } else {
        &bytes
    };
    Ok(String::from_utf8_lossy(clipped).to_string())
}

pub fn list_sandbox_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }

    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

pub fn sandbox_list(dir: &Path) -> anyhow::Result<Vec<String>> {
    let base = canonicalize_dir(dir)?;
    let mut items = Vec::new();
    for path in list_sandbox_files(&base) {
        if let Ok(rel) = path.strip_prefix(&base) {
            items.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(items)
}

pub fn sandbox_read(dir: &Path, rel_path: &str, max_bytes: usize) -> anyhow::Result<String> {
    let rel = parse_rel_path(rel_path)?;
    let base = canonicalize_dir(dir)?;
    let target = ensure_path_within_dir(&base, &base.join(rel))?;
    read_text_file(&target, max_bytes)
}

pub fn sandbox_write(dir: &Path, rel_path: &str, contents: &str) -> anyhow::Result<PathBuf> {
    let rel = parse_rel_path(rel_path)?;
    let base = canonicalize_dir(dir)?;
    let target = ensure_path_within_dir(&base, &base.join(rel))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    fs::write(&target, contents).with_context(|| format!("write {}", target.display()))?;
    Ok(target)
}

pub fn sandbox_append(dir: &Path, rel_path: &str, contents: &str) -> anyhow::Result<PathBuf> {
    let rel = parse_rel_path(rel_path)?;
    let base = canonicalize_dir(dir)?;
    let target = ensure_path_within_dir(&base, &base.join(rel))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let mut existing = if target.exists() {
        fs::read_to_string(&target).with_context(|| format!("read {}", target.display()))?
    } else {
        String::new()
    };
    existing.push_str(contents);
    fs::write(&target, existing).with_context(|| format!("write {}", target.display()))?;
    Ok(target)
}

pub fn truncate_for_ui(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in text.chars().take(max_chars) {
        out.push(ch);
    }
    if text.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

pub fn build_recent_chat_prompt_context(
    messages: &[(String, String)],
    max_messages: usize,
    max_chars: usize,
) -> String {
    let mut lines = Vec::new();
    for (speaker, content) in messages
        .iter()
        .rev()
        .take(max_messages)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let content = truncate_for_ui(content.trim(), 900);
        if !content.trim().is_empty() {
            lines.push(format!("{}: {}", speaker.trim(), content));
        }
    }
    truncate_for_ui(&lines.join("\n"), max_chars)
}

pub fn build_sandbox_prompt_context(
    dir: Option<&Path>,
    scratchpad_rel_path: &str,
    ledger_rel_path: &str,
) -> String {
    let Some(dir) = dir else {
        return "Sandbox folder is not available right now.".to_string();
    };

    let mut lines = vec![format!("Root: {SANDBOX_DIR_NAME}/")];
    if let Ok(items) = sandbox_list(dir) {
        if items.is_empty() {
            lines.push("Files: (sandbox is currently empty)".to_string());
        } else {
            lines.push("Files:".to_string());
            for item in items.iter().take(40) {
                lines.push(format!("- {item}"));
            }
            if items.len() > 40 {
                lines.push(format!("- ...and {} more", items.len() - 40));
            }
        }
    }

    if let Ok(scratchpad) = sandbox_read(dir, scratchpad_rel_path, 30_000) {
        let scratchpad = scratchpad.trim();
        if !scratchpad.is_empty() {
            lines.push(String::new());
            lines.push(format!("Scratchpad (`{scratchpad_rel_path}`):"));
            lines.push(truncate_for_ui(scratchpad, 8_000));
        }
    }

    if let Ok(ledger) = sandbox_read(dir, ledger_rel_path, 24_000) {
        let ledger = ledger.trim();
        if !ledger.is_empty() {
            lines.push(String::new());
            lines.push(format!("Task ledger (`{ledger_rel_path}`):"));
            lines.push(truncate_for_ui(ledger, 6_000));
        }
    }

    lines.join("\n")
}

pub fn message_looks_multistep(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.lines().count() >= 3 || trimmed.len() >= 220 {
        return true;
    }

    let lower = trimmed.to_lowercase();
    let keywords = [
        "plan",
        "steps",
        "checklist",
        "workflow",
        "first",
        "then",
        "after",
        "next",
        "compare",
        "organize",
        "coordinate",
        "research",
        "summarize",
        "write",
        "create",
        "review",
        "analyze",
        "prepare",
        "save",
        "track",
    ];
    let keyword_hits = keywords
        .iter()
        .filter(|keyword| lower.contains(**keyword))
        .count();
    let conjunction_hits = [" and ", " then ", " after ", " also ", " while "]
        .iter()
        .filter(|token| lower.contains(**token))
        .count();

    keyword_hits >= 3 || (keyword_hits >= 2 && conjunction_hits >= 1)
}

pub fn task_ledger_has_real_content(dir: Option<&Path>) -> bool {
    let Some(dir) = dir else {
        return false;
    };
    let Ok(text) = sandbox_read(dir, DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH, 24_000) else {
        return false;
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let placeholder_markers = [
        "Capture the current task here.",
        "Record the next concrete step here.",
        "## Open Questions\n- none right now",
        "## Files Touched\n- none yet",
        "## Working Notes\n- none yet",
    ];
    !placeholder_markers
        .iter()
        .all(|marker| trimmed.contains(marker))
}

pub fn build_task_ledger_prompt_nudge(prompt: &str, dir: Option<&Path>) -> Option<String> {
    if !message_looks_multistep(prompt) {
        return None;
    }
    Some(if task_ledger_has_real_content(dir) {
        "This looks like a multi-step task. Before and during the task, prefer `sandbox.preload` with `include_ledger:true` so you can inspect the latest scratchpad + task ledger, and use `sandbox.ledger` whenever the current task, next step, open questions, or files touched meaningfully change.".to_string()
    } else {
        "This looks like a multi-step task and the task ledger appears empty or generic. Prefer starting with `sandbox.preload` (include the scratchpad, task ledger, and any relevant files), then use `sandbox.ledger` to record the current task, next step, open questions, files touched, and durable working notes before you continue.".to_string()
    })
}

pub fn build_task_ledger_user_hint(prompt: &str, dir: Option<&Path>) -> Option<String> {
    if !message_looks_multistep(prompt) {
        return None;
    }
    Some(if task_ledger_has_real_content(dir) {
        "This looks multi-step. Chatty-EDU may use `sandbox.preload` + `sandbox.ledger` so it can keep a clean task record while it works. `Approve + Continue` is usually the smoothest path.".to_string()
    } else {
        "This looks multi-step. Chatty-EDU may want to initialize the task ledger, preload the sandbox, and continue after approval so longer work stays grounded.".to_string()
    })
}

pub fn render_task_ledger_markdown(
    status: &str,
    current_task: &str,
    next_step: &str,
    open_questions: &[String],
    files_touched: &[String],
    notes: &[String],
) -> String {
    let mut lines = vec![
        "# Chatty-EDU Task Ledger".to_string(),
        format!("Updated: {}", chrono::Utc::now().timestamp_millis().max(0)),
        format!(
            "Status: {}",
            if status.trim().is_empty() {
                "active"
            } else {
                status.trim()
            }
        ),
        String::new(),
        "## Current Task".to_string(),
        if current_task.trim().is_empty() {
            "(not set)".to_string()
        } else {
            current_task.trim().to_string()
        },
        String::new(),
        "## Next Step".to_string(),
        if next_step.trim().is_empty() {
            "(not set)".to_string()
        } else {
            next_step.trim().to_string()
        },
        String::new(),
        "## Open Questions".to_string(),
    ];
    if open_questions.is_empty() {
        lines.push("- none right now".to_string());
    } else {
        for item in open_questions {
            lines.push(format!("- {}", item.trim()));
        }
    }
    lines.push(String::new());
    lines.push("## Files Touched".to_string());
    if files_touched.is_empty() {
        lines.push("- none yet".to_string());
    } else {
        for item in files_touched {
            lines.push(format!("- {}", item.trim()));
        }
    }
    lines.push(String::new());
    lines.push("## Working Notes".to_string());
    if notes.is_empty() {
        lines.push("- none yet".to_string());
    } else {
        for item in notes {
            lines.push(format!("- {}", item.trim()));
        }
    }
    lines.join("\n")
}

pub fn read_task_ledger_summary(dir: &Path) -> Option<TaskLedgerSummary> {
    let text = sandbox_read(dir, DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH, 24_000).ok()?;
    let mut summary = TaskLedgerSummary::default();

    enum Section {
        None,
        CurrentTask,
        NextStep,
        OpenQuestions,
        FilesTouched,
        WorkingNotes,
    }

    let mut section = Section::None;
    let mut current_task_lines = Vec::new();
    let mut next_step_lines = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("Status:") {
            summary.status = rest.trim().to_string();
            continue;
        }

        section = match trimmed {
            "## Current Task" => Section::CurrentTask,
            "## Next Step" => Section::NextStep,
            "## Open Questions" => Section::OpenQuestions,
            "## Files Touched" => Section::FilesTouched,
            "## Working Notes" => Section::WorkingNotes,
            _ if trimmed.starts_with("## ") => Section::None,
            _ => section,
        };

        if matches!(
            trimmed,
            "## Current Task"
                | "## Next Step"
                | "## Open Questions"
                | "## Files Touched"
                | "## Working Notes"
        ) {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }

        match section {
            Section::CurrentTask => {
                if trimmed != "(not set)" {
                    current_task_lines.push(trimmed.to_string());
                }
            }
            Section::NextStep => {
                if trimmed != "(not set)" {
                    next_step_lines.push(trimmed.to_string());
                }
            }
            Section::OpenQuestions => {
                if let Some(item) = trimmed.strip_prefix("- ") {
                    if item.trim() != "none right now" {
                        summary.open_questions.push(item.trim().to_string());
                    }
                }
            }
            Section::FilesTouched => {
                if let Some(item) = trimmed.strip_prefix("- ") {
                    if item.trim() != "none yet" {
                        summary.files_touched.push(item.trim().to_string());
                    }
                }
            }
            Section::WorkingNotes => {
                if let Some(item) = trimmed.strip_prefix("- ") {
                    if item.trim() != "none yet" {
                        summary.notes.push(item.trim().to_string());
                    }
                }
            }
            Section::None => {}
        }
    }

    summary.current_task = current_task_lines.join(" ");
    summary.next_step = next_step_lines.join(" ");
    Some(summary)
}

pub fn sandbox_write_task_ledger(
    dir: &Path,
    status: &str,
    current_task: &str,
    next_step: &str,
    open_questions: &[String],
    files_touched: &[String],
    notes: &[String],
) -> anyhow::Result<PathBuf> {
    let markdown = render_task_ledger_markdown(
        status,
        current_task,
        next_step,
        open_questions,
        files_touched,
        notes,
    );
    sandbox_write(dir, DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH, &markdown)
}

pub fn ensure_default_sandbox_scratchpad_file(dir: &Path) -> anyhow::Result<PathBuf> {
    let rel = parse_rel_path(DEFAULT_SANDBOX_SCRATCHPAD_REL_PATH)?;
    let base = canonicalize_dir(dir)?;
    let target = base.join(rel);
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("missing scratchpad parent dir"))?;
    fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    if !target.exists() {
        fs::write(&target, "").with_context(|| format!("create {}", target.display()))?;
    }
    ensure_path_within_dir(&base, &target)
}

pub fn ensure_default_sandbox_task_ledger_file(dir: &Path) -> anyhow::Result<PathBuf> {
    let rel = parse_rel_path(DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH)?;
    let base = canonicalize_dir(dir)?;
    let target = base.join(rel);
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("missing task ledger parent dir"))?;
    fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    if !target.exists() {
        let initial = render_task_ledger_markdown(
            "idle",
            "Capture the current task here.",
            "Record the next concrete step here.",
            &[],
            &[],
            &[],
        );
        fs::write(&target, initial).with_context(|| format!("create {}", target.display()))?;
    }
    ensure_path_within_dir(&base, &target)
}

pub fn sandbox_preload(
    dir: &Path,
    paths: &[String],
    include_list: bool,
    include_scratchpad: bool,
    include_ledger: bool,
    note: &str,
) -> anyhow::Result<SandboxPreloadResult> {
    let mut lines = vec!["sandbox.preload succeeded.".to_string()];
    if !note.trim().is_empty() {
        lines.push(format!("Reason: {}", note.trim()));
    }

    let mut loaded_count = 0usize;
    if include_list {
        let items = sandbox_list(dir)?;
        lines.push(format!(
            "Sandbox file index:\n{}",
            if items.is_empty() {
                "(empty)".to_string()
            } else {
                items
                    .into_iter()
                    .take(60)
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        ));
        loaded_count += 1;
    }
    if include_scratchpad {
        let _ = ensure_default_sandbox_scratchpad_file(dir)?;
        let text = sandbox_read(dir, DEFAULT_SANDBOX_SCRATCHPAD_REL_PATH, 24_000)?;
        lines.push(format!(
            "Scratchpad (`{}`):\n{}",
            DEFAULT_SANDBOX_SCRATCHPAD_REL_PATH,
            if text.trim().is_empty() {
                "(empty)".to_string()
            } else {
                truncate_for_ui(text.trim(), 6_000)
            }
        ));
        loaded_count += 1;
    }
    if include_ledger {
        let _ = ensure_default_sandbox_task_ledger_file(dir)?;
        let text = sandbox_read(dir, DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH, 24_000)?;
        lines.push(format!(
            "Task ledger (`{}`):\n{}",
            DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH,
            if text.trim().is_empty() {
                "(empty)".to_string()
            } else {
                truncate_for_ui(text.trim(), 6_000)
            }
        ));
        loaded_count += 1;
    }
    for path in paths {
        let text = sandbox_read(dir, path, 30_000)?;
        lines.push(format!(
            "File (`{}`):\n{}",
            path.trim(),
            if text.trim().is_empty() {
                "(empty)".to_string()
            } else {
                truncate_for_ui(text.trim(), 6_000)
            }
        ));
        loaded_count += 1;
    }

    Ok(SandboxPreloadResult {
        prompt_block: lines.join("\n\n"),
        loaded_count,
    })
}

pub fn extract_sandbox_actions_from_text(text: &str) -> Vec<SandboxAction> {
    #[derive(serde::Deserialize)]
    struct ToolReq {
        tool: String,
        path: Option<String>,
        paths: Option<Vec<String>>,
        contents: Option<String>,
        include_list: Option<bool>,
        include_scratchpad: Option<bool>,
        include_ledger: Option<bool>,
        note: Option<String>,
        status: Option<String>,
        current_task: Option<String>,
        next_step: Option<String>,
        open_questions: Option<Vec<String>>,
        files_touched: Option<Vec<String>>,
        notes: Option<Vec<String>>,
    }

    fn parse_obj(s: &str) -> Option<ToolReq> {
        let s = s.trim();
        if !s.starts_with('{') || !s.ends_with('}') {
            return None;
        }
        serde_json::from_str::<ToolReq>(s).ok()
    }

    fn actions_from_req(req: ToolReq) -> Option<SandboxAction> {
        match req.tool.as_str() {
            "sandbox.write" => Some(SandboxAction::Write {
                path: req.path?,
                contents: req.contents.unwrap_or_default(),
            }),
            "sandbox.append" => Some(SandboxAction::Append {
                path: req.path?,
                contents: req.contents.unwrap_or_default(),
            }),
            "sandbox.read" => Some(SandboxAction::Read { path: req.path? }),
            "sandbox.list" => Some(SandboxAction::List),
            "sandbox.preload" => Some(SandboxAction::Preload {
                paths: req
                    .paths
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect(),
                include_list: req.include_list.unwrap_or(true),
                include_scratchpad: req.include_scratchpad.unwrap_or(true),
                include_ledger: req.include_ledger.unwrap_or(true),
                note: req.note.unwrap_or_default().trim().to_string(),
            }),
            "sandbox.ledger" => Some(SandboxAction::Ledger {
                status: req.status.unwrap_or_default().trim().to_string(),
                current_task: req.current_task.unwrap_or_default().trim().to_string(),
                next_step: req.next_step.unwrap_or_default().trim().to_string(),
                open_questions: req
                    .open_questions
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect(),
                files_touched: req
                    .files_touched
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect(),
                notes: req
                    .notes
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect(),
            }),
            _ => None,
        }
    }

    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_matches('`');
        if let Some(req) = parse_obj(line) {
            if let Some(action) = actions_from_req(req) {
                out.push(action);
            }
        }
    }
    if !out.is_empty() {
        return out;
    }

    let needle = "\"tool\":\"sandbox.";
    let mut i = 0usize;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let Some(pos) = text[i..].find(needle) else {
            break;
        };
        let pos = i + pos;

        let mut l = pos;
        while l > 0 && bytes[l] != b'{' {
            l -= 1;
        }
        if bytes.get(l) != Some(&b'{') {
            i = pos + needle.len();
            continue;
        }

        let mut r = l;
        let mut depth = 0i32;
        let mut in_str = false;
        let mut esc = false;
        while r < bytes.len() {
            let ch = bytes[r] as char;
            if in_str {
                if esc {
                    esc = false;
                } else if ch == '\\' {
                    esc = true;
                } else if ch == '"' {
                    in_str = false;
                }
            } else if ch == '"' {
                in_str = true;
            } else if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    r += 1;
                    break;
                }
            }
            r += 1;
        }
        if depth != 0 || r <= l {
            i = pos + needle.len();
            continue;
        }
        let candidate = text[l..r].trim();
        if let Some(req) = parse_obj(candidate) {
            if let Some(action) = actions_from_req(req) {
                out.push(action);
            }
        }
        i = r;
    }
    out
}
