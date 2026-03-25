use crate::homework_pack::{HomeworkAssignment, HomeworkPack, HomeworkSubmission};
use chrono::Utc;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct PackMdDefaults {
    pub version: String,
    pub school_id: String,
    pub class_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct OutgoingTranscribeReport {
    pub processed: usize,
    pub written: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<String>,
    pub outputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct MarkingTranscribeReport {
    pub processed: usize,
    pub written: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<String>,
    pub outputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct PackExportReport {
    pub processed: usize,
    pub written: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<String>,
    pub outputs: Vec<PathBuf>,
}

pub fn homework_outgoing_dir(base: &Path) -> PathBuf {
    base.join("homework").join("outgoing")
}

pub fn homework_marking_dir(base: &Path) -> PathBuf {
    base.join("homework").join("marking")
}

pub fn homework_printables_dir(base: &Path) -> PathBuf {
    base.join("homework").join("printables")
}

pub fn homework_rubrics_dir(base: &Path) -> PathBuf {
    base.join("homework").join("rubrics")
}

fn iso_now() -> String {
    Utc::now().to_rfc3339()
}

fn parse_key_value_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (k, v) = trimmed.split_once(':')?;
    let key = k.trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some((key, v.trim().to_string()))
}

fn normalize_metadata_key(key: &str) -> String {
    key.trim()
        .to_ascii_lowercase()
        .replace(' ', "_")
        .replace('-', "_")
}

fn is_year_level_metadata_key(key: &str) -> bool {
    matches!(
        normalize_metadata_key(key).as_str(),
        "year"
            | "year_level"
            | "yearlevel"
            | "year_group"
            | "yeargroup"
            | "grade"
            | "grade_level"
            | "gradelevel"
    )
}

fn parse_bool(value: &str) -> Option<bool> {
    let v = value.trim().to_ascii_lowercase();
    match v.as_str() {
        "true" | "yes" | "y" | "1" | "on" => Some(true),
        "false" | "no" | "n" | "0" | "off" => Some(false),
        _ => None,
    }
}

fn parse_assignment_heading(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("## ") {
        return None;
    }
    let rest = trimmed.trim_start_matches("## ").trim();
    let rest_lower = rest.to_ascii_lowercase();
    if !rest_lower.starts_with("assignment") {
        return None;
    }
    let after = rest
        .get("assignment".len()..)
        .unwrap_or_default()
        .trim()
        .trim_start_matches(':')
        .trim();
    if after.is_empty() {
        return None;
    }

    let (id, title) = if let Some((l, r)) = after.split_once('|') {
        (l.trim(), r.trim())
    } else if let Some((l, r)) = after.split_once(':') {
        (l.trim(), r.trim())
    } else if let Some(idx) = after.find(" - ") {
        (after[..idx].trim(), after[idx + 3..].trim())
    } else if let Some(idx) = after.find(" — ") {
        (after[..idx].trim(), after[idx + 3..].trim())
    } else if let Some(idx) = after.find(" – ") {
        (after[..idx].trim(), after[idx + 3..].trim())
    } else {
        (after.trim(), "")
    };

    if id.is_empty() {
        return None;
    }
    Some((id.to_string(), title.to_string()))
}

fn is_instructions_heading(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower == "### instructions"
        || lower == "## instructions"
        || lower == "# instructions"
        || lower == "instructions:"
        || lower == "instructions"
}

fn is_student_printable_heading(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower == "### student printable"
        || lower == "## student printable"
        || lower == "# student printable"
        || lower == "student printable:"
        || lower == "student printable"
        || lower == "### printable"
        || lower == "## printable"
        || lower == "# printable"
        || lower == "printable:"
        || lower == "printable"
        || lower == "### student handout"
        || lower == "student handout"
        || lower == "handout:"
        || lower == "handout"
}

fn is_teacher_rubric_heading(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower == "### rubric"
        || lower == "## rubric"
        || lower == "# rubric"
        || lower == "rubric:"
        || lower == "rubric"
        || lower == "### marking guide"
        || lower == "## marking guide"
        || lower == "# marking guide"
        || lower == "marking guide:"
        || lower == "marking guide"
        || lower == "### teacher notes"
        || lower == "## teacher notes"
        || lower == "# teacher notes"
        || lower == "teacher notes:"
        || lower == "teacher notes"
}

fn parse_list_item(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("- ") {
        Some(trimmed[2..].trim().to_string())
    } else if trimmed.starts_with("* ") {
        Some(trimmed[2..].trim().to_string())
    } else {
        None
    }
}

fn is_assignment_metadata_key(key: &str) -> bool {
    is_year_level_metadata_key(key)
        || matches!(
            normalize_metadata_key(key).as_str(),
            "subject"
                | "due"
                | "due_at"
                | "allow_games"
                | "allow_ai_premark"
                | "max_score"
                | "attachments"
        )
}

#[derive(Debug, Clone, Default)]
struct AssignmentDraft {
    id: String,
    title: String,
    subject: String,
    year_level: String,
    due_at: Option<String>,
    allow_games: bool,
    allow_ai_premark: bool,
    max_score: Option<i32>,
    attachments: Vec<String>,
    instructions_lines: Vec<String>,
    student_printable_lines: Vec<String>,
    rubric_lines: Vec<String>,
}

fn finalize_assignment(draft: AssignmentDraft) -> Result<HomeworkAssignment, String> {
    if draft.id.trim().is_empty() {
        return Err("assignment missing id".to_string());
    }

    let subject = if draft.subject.trim().is_empty() {
        "General".to_string()
    } else {
        draft.subject.trim().to_string()
    };
    let year_level = if draft.year_level.trim().is_empty() {
        "7".to_string()
    } else {
        draft.year_level.trim().to_string()
    };

    let instructions_md = draft.instructions_lines.join("\n").trim_end().to_string();
    let student_printable_md = draft
        .student_printable_lines
        .join("\n")
        .trim_end()
        .to_string();
    let teacher_rubric_md = draft.rubric_lines.join("\n").trim_end().to_string();

    Ok(HomeworkAssignment {
        id: draft.id.trim().to_string(),
        title: draft.title.trim().to_string(),
        subject,
        year_level,
        due_at: draft
            .due_at
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        instructions_md,
        student_printable_md: if student_printable_md.trim().is_empty() {
            None
        } else {
            Some(student_printable_md)
        },
        teacher_rubric_md: if teacher_rubric_md.trim().is_empty() {
            None
        } else {
            Some(teacher_rubric_md)
        },
        attachments: draft
            .attachments
            .into_iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect(),
        allow_games: draft.allow_games,
        allow_ai_premark: draft.allow_ai_premark,
        max_score: draft.max_score,
    })
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum BodySection {
    None,
    Instructions,
    StudentPrintable,
    TeacherRubric,
}

pub fn parse_homework_pack_markdown(
    markdown: &str,
    defaults: &PackMdDefaults,
) -> Result<HomeworkPack, String> {
    let mut version = defaults.version.clone();
    let mut school_id = defaults.school_id.clone();
    let mut class_id = defaults.class_id.clone();
    let mut created_at: Option<String> = None;

    let mut assignments: Vec<HomeworkAssignment> = Vec::new();
    let mut current: Option<AssignmentDraft> = None;
    let mut section = BodySection::None;
    let mut reading_attachments = false;
    let mut fenced_section: Option<BodySection> = None;

    for (idx, line) in markdown.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();

        if let (Some(draft), Some(fenced)) = (current.as_mut(), fenced_section) {
            if trimmed.starts_with("```") {
                fenced_section = None;
                continue;
            }

            match fenced {
                BodySection::Instructions => {
                    if is_student_printable_heading(line) {
                        fenced_section = None;
                        section = BodySection::StudentPrintable;
                        continue;
                    }
                    if is_teacher_rubric_heading(line) {
                        fenced_section = None;
                        section = BodySection::TeacherRubric;
                        continue;
                    }
                    draft.instructions_lines.push(line.to_string());
                    continue;
                }
                BodySection::StudentPrintable => {
                    if is_teacher_rubric_heading(line) {
                        fenced_section = None;
                        section = BodySection::TeacherRubric;
                        continue;
                    }
                    if is_instructions_heading(line)
                        || is_student_printable_heading(line)
                        || parse_assignment_heading(line).is_some()
                    {
                        continue;
                    }
                    if let Some((key, _)) = parse_key_value_line(line) {
                        if is_assignment_metadata_key(&key) {
                            continue;
                        }
                    }
                    draft.student_printable_lines.push(line.to_string());
                    continue;
                }
                BodySection::TeacherRubric => {
                    if is_student_printable_heading(line) {
                        fenced_section = None;
                        section = BodySection::StudentPrintable;
                        continue;
                    }
                    draft.rubric_lines.push(line.to_string());
                    continue;
                }
                BodySection::None => {
                    fenced_section = None;
                }
            }
        }

        if trimmed.starts_with("```") {
            if section != BodySection::None && current.is_some() {
                fenced_section = Some(section);
            }
            continue;
        }

        if let Some((id, title)) = parse_assignment_heading(line) {
            if let Some(draft) = current.take() {
                let a = finalize_assignment(draft)
                    .map_err(|e| format!("Line {line_no}: could not finalize assignment: {e}"))?;
                assignments.push(a);
            }
            current = Some(AssignmentDraft {
                id,
                title,
                subject: String::new(),
                year_level: String::new(),
                due_at: None,
                allow_games: false,
                allow_ai_premark: true,
                max_score: None,
                attachments: Vec::new(),
                instructions_lines: Vec::new(),
                student_printable_lines: Vec::new(),
                rubric_lines: Vec::new(),
            });
            section = BodySection::None;
            reading_attachments = false;
            continue;
        }

        let Some(draft) = current.as_mut() else {
            if let Some((key, value)) = parse_key_value_line(line) {
                match key.trim().to_ascii_lowercase().as_str() {
                    "version" => {
                        if !value.trim().is_empty() {
                            version = value.trim().to_string();
                        }
                    }
                    "school_id" => {
                        if !value.trim().is_empty() {
                            school_id = value.trim().to_string();
                        }
                    }
                    "class_id" => {
                        if !value.trim().is_empty() {
                            class_id = value.trim().to_string();
                        }
                    }
                    "created_at" => {
                        if !value.trim().is_empty() {
                            created_at = Some(value.trim().to_string());
                        }
                    }
                    _ => {}
                }
            }
            continue;
        };

        if reading_attachments {
            if let Some(item) = parse_list_item(line) {
                if !item.is_empty() {
                    draft.attachments.push(item);
                }
                continue;
            }
            if line.trim().is_empty() {
                reading_attachments = false;
                continue;
            }
            // Stop reading attachments and fall through to treat this line normally.
            reading_attachments = false;
        }

        if is_instructions_heading(line) {
            section = BodySection::Instructions;
            continue;
        }

        if is_student_printable_heading(line) {
            section = BodySection::StudentPrintable;
            continue;
        }

        if is_teacher_rubric_heading(line) {
            section = BodySection::TeacherRubric;
            continue;
        }

        match section {
            BodySection::Instructions => {
                draft.instructions_lines.push(line.to_string());
                continue;
            }
            BodySection::StudentPrintable => {
                draft.student_printable_lines.push(line.to_string());
                continue;
            }
            BodySection::TeacherRubric => {
                draft.rubric_lines.push(line.to_string());
                continue;
            }
            BodySection::None => {}
        }

        if let Some((key, value)) = parse_key_value_line(line) {
            let normalized_key = normalize_metadata_key(&key);
            match normalized_key.as_str() {
                "subject" => draft.subject = value,
                "year" | "year_level" | "yearlevel" | "year_group" | "yeargroup" | "grade"
                | "grade_level" | "gradelevel" => draft.year_level = value,
                "due_at" | "due" => {
                    if value.trim().is_empty() {
                        draft.due_at = None;
                    } else {
                        draft.due_at = Some(value);
                    }
                }
                "allow_games" => {
                    if let Some(v) = parse_bool(&value) {
                        draft.allow_games = v;
                    }
                }
                "allow_ai_premark" => {
                    if let Some(v) = parse_bool(&value) {
                        draft.allow_ai_premark = v;
                    }
                }
                "max_score" => {
                    draft.max_score = value.trim().parse::<i32>().ok();
                }
                "attachments" => {
                    if value.trim().is_empty() {
                        reading_attachments = true;
                    } else {
                        draft.attachments.extend(
                            value
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty()),
                        );
                    }
                }
                _ => {}
            }
            continue;
        }

        if line.trim().is_empty() {
            continue;
        }

        // If we get here: it's non-empty and not a metadata line; treat the rest as instructions.
        section = BodySection::Instructions;
        draft.instructions_lines.push(line.to_string());
    }

    if let Some(draft) = current.take() {
        let a = finalize_assignment(draft)
            .map_err(|e| format!("Could not finalize assignment: {e}"))?;
        assignments.push(a);
    }

    if assignments.is_empty() {
        return Err(
            "No assignments found. Use headings like '## Assignment: hw-001 | Title'.".to_string(),
        );
    }

    Ok(HomeworkPack {
        version: if version.trim().is_empty() {
            "1.0".to_string()
        } else {
            version
        },
        school_id: if school_id.trim().is_empty() {
            "school".to_string()
        } else {
            school_id
        },
        class_id: if class_id.trim().is_empty() {
            "class".to_string()
        } else {
            class_id
        },
        created_at: created_at.unwrap_or_else(iso_now),
        assignments,
    })
}

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("homework_pack")
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

fn safe_component(text: &str) -> String {
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

fn write_pack_json(assigned_dir: &Path, stem: &str, pack: &HomeworkPack) -> io::Result<PathBuf> {
    fs::create_dir_all(assigned_dir)?;
    let mut file_stem = stem.to_string();
    if !file_stem.to_ascii_lowercase().contains("homework_pack") {
        file_stem = format!("homework_pack_{file_stem}");
    }
    let path = assigned_dir.join(format!("{file_stem}.json"));
    let json = serde_json::to_string_pretty(pack)?;
    fs::write(&path, json)?;
    Ok(path)
}

fn pack_json_path(assigned_dir: &Path, stem: &str) -> PathBuf {
    let mut file_stem = stem.to_string();
    if !file_stem.to_ascii_lowercase().contains("homework_pack") {
        file_stem = format!("homework_pack_{file_stem}");
    }
    assigned_dir.join(format!("{file_stem}.json"))
}

pub fn transcribe_outgoing_packs(
    base: &Path,
    defaults: &PackMdDefaults,
) -> io::Result<OutgoingTranscribeReport> {
    let outgoing_dir = homework_outgoing_dir(base);
    let assigned_dir = base.join("homework").join("assigned");
    fs::create_dir_all(&outgoing_dir)?;
    fs::create_dir_all(&assigned_dir)?;

    let mut report = OutgoingTranscribeReport::default();

    let entries = match fs::read_dir(&outgoing_dir) {
        Ok(v) => v,
        Err(err) => {
            report.failed += 1;
            report.errors.push(format!(
                "Could not read outgoing dir {}: {err}",
                outgoing_dir.display()
            ));
            return Ok(report);
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.') || n.eq_ignore_ascii_case("gitkeep"))
            .unwrap_or(false)
        {
            continue;
        }
        if path
            .extension()
            .and_then(OsStr::to_str)
            .map(|e| e.eq_ignore_ascii_case("md"))
            .unwrap_or(false)
            == false
        {
            continue;
        }

        report.processed += 1;
        let contents = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(err) => {
                report.failed += 1;
                report
                    .errors
                    .push(format!("{}: could not read: {err}", path.display()));
                continue;
            }
        };

        let pack = match parse_homework_pack_markdown(&contents, defaults) {
            Ok(p) => p,
            Err(err) => {
                report.failed += 1;
                report
                    .errors
                    .push(format!("{}: parse error: {err}", path.display()));
                continue;
            }
        };

        let stem = safe_stem(&path);
        let dest = pack_json_path(&assigned_dir, &stem);
        if dest.exists() {
            let src_time = fs::metadata(&path).and_then(|m| m.modified()).ok();
            let dest_time = fs::metadata(&dest).and_then(|m| m.modified()).ok();
            let should_overwrite = match (src_time, dest_time) {
                (Some(s), Some(d)) => s > d,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => false,
            };
            if !should_overwrite {
                report.skipped += 1;
                continue;
            }
        }

        match write_pack_json(&assigned_dir, &stem, &pack) {
            Ok(out_path) => {
                report.written += 1;
                report.outputs.push(out_path);
            }
            Err(err) => {
                report.failed += 1;
                report
                    .errors
                    .push(format!("{}: could not write json: {err}", path.display()));
            }
        }
    }

    Ok(report)
}

fn find_assignment_for_submission<'a>(
    pack: Option<&'a HomeworkPack>,
    submission: &HomeworkSubmission,
) -> Option<&'a HomeworkAssignment> {
    let pack = pack?;
    pack.assignments
        .iter()
        .find(|a| a.id == submission.assignment_id)
}

pub fn submission_to_marking_markdown(
    submission: &HomeworkSubmission,
    assignment: Option<&HomeworkAssignment>,
) -> String {
    let title = assignment
        .map(|a| a.title.as_str())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(&submission.assignment_id);

    let mut out = String::new();
    out.push_str("# Marking sheet\n\n");
    out.push_str(&format!(
        "- Assignment: **{}** (`{}`)\n",
        title, submission.assignment_id
    ));
    out.push_str(&format!(
        "- Student: **{}** (`{}`)\n",
        submission.student_name, submission.student_id
    ));
    out.push_str(&format!("- Class: `{}`\n", submission.class_id));
    out.push_str(&format!("- Submitted at: `{}`\n", submission.submitted_at));
    if let Some(h) = &submission.final_hash {
        out.push_str(&format!("- Final hash: `{}`\n", h));
    }
    if !submission.attachments.is_empty() {
        out.push_str("- Attachments:\n");
        for a in &submission.attachments {
            out.push_str(&format!("  - `{}`\n", a));
        }
    }

    if let Some(a) = assignment {
        let instr = assignment_printable_md(a);
        if !instr.trim().is_empty() {
            out.push_str("\n## Instructions\n\n");
            out.push_str(instr.trim_end());
            out.push('\n');
        }
        if let Some(max) = a.max_score {
            out.push_str(&format!("\n- Max score: **{}**\n", max));
        }
        if let Some(rubric) = a
            .teacher_rubric_md
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            out.push_str("\n## Rubric / Marking guide\n\n");
            out.push_str(rubric.trim_end());
            out.push('\n');
        }
    }

    out.push_str("\n## Student work\n\n");
    if let Some(text) = submission.answers_text.as_ref() {
        if !text.trim().is_empty() {
            out.push_str(text.trim_end());
            out.push('\n');
        }
    }
    if !submission.answers.is_empty() {
        out.push('\n');
        for (idx, a) in submission.answers.iter().enumerate() {
            out.push_str(&format!("### Q{}\n\n", idx + 1));
            out.push_str(&format!("**Question:** {}\n\n", a.question.trim()));
            out.push_str(&format!("**Response:** {}\n\n", a.response.trim()));
        }
    }

    if let Some(premark) = &submission.ai_premark {
        if premark.score.is_some()
            || premark
                .feedback
                .as_ref()
                .is_some_and(|f| !f.trim().is_empty())
        {
            out.push_str("\n## AI pre-mark (optional)\n\n");
            if let Some(score) = premark.score {
                out.push_str(&format!("- Score: **{}**\n", score));
            }
            if let Some(feedback) = &premark.feedback {
                if !feedback.trim().is_empty() {
                    out.push_str("\n**Feedback:**\n\n");
                    out.push_str(feedback.trim_end());
                    out.push('\n');
                }
            }
        }
    }

    out.push_str("\n## Teacher marking\n\n");
    out.push_str("- Score: ____\n");
    out.push_str("- Comments:\n");
    out.push_str("  - \n");

    out
}

fn assignment_printable_md(assignment: &HomeworkAssignment) -> &str {
    assignment
        .student_printable_md
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(assignment.instructions_md.as_str())
}

pub fn export_student_printables(base: &Path, pack: &HomeworkPack) -> io::Result<PackExportReport> {
    let dir = homework_printables_dir(base);
    fs::create_dir_all(&dir)?;

    let mut report = PackExportReport::default();
    for a in &pack.assignments {
        report.processed += 1;
        let id = safe_component(&a.id);
        let out_path = dir.join(format!("student_{id}.md"));

        let mut md = String::new();
        let title = if a.title.trim().is_empty() {
            a.id.trim()
        } else {
            a.title.trim()
        };
        md.push_str(&format!("# {}\n\n", title));
        md.push_str(&format!("- Assignment: `{}`\n", a.id.trim()));
        md.push_str(&format!("- Class: `{}`\n", pack.class_id));
        md.push_str(&format!("- Subject: `{}`\n", a.subject));
        md.push_str(&format!("- Year level: `{}`\n", a.year_level));
        md.push_str(&format!("- Due: `{}`\n", a.due_at.as_deref().unwrap_or("")));
        md.push_str("\nStudent name: ______________________\n\n");
        md.push_str("Student ID: ________________________\n\n");

        if !a.attachments.is_empty() {
            md.push_str("## Attachments\n\n");
            for att in &a.attachments {
                md.push_str(&format!("- `{}`\n", att));
            }
            md.push('\n');
        }

        md.push_str("## Instructions\n\n");
        md.push_str(assignment_printable_md(a).trim_end());
        md.push('\n');

        match fs::write(&out_path, md) {
            Ok(_) => {
                report.written += 1;
                report.outputs.push(out_path);
            }
            Err(err) => {
                report.failed += 1;
                report
                    .errors
                    .push(format!("{}: could not write: {err}", out_path.display()));
            }
        }
    }

    Ok(report)
}

pub fn export_teacher_rubrics(base: &Path, pack: &HomeworkPack) -> io::Result<PackExportReport> {
    let dir = homework_rubrics_dir(base);
    fs::create_dir_all(&dir)?;

    let mut report = PackExportReport::default();
    for a in &pack.assignments {
        report.processed += 1;
        let id = safe_component(&a.id);
        let out_path = dir.join(format!("rubric_{id}.md"));

        let mut md = String::new();
        let title = if a.title.trim().is_empty() {
            a.id.trim()
        } else {
            a.title.trim()
        };
        md.push_str(&format!("# Rubric / Marking guide: {}\n\n", title));
        md.push_str(&format!("- Assignment: `{}`\n", a.id.trim()));
        md.push_str(&format!("- Class: `{}`\n", pack.class_id));
        md.push_str(&format!("- Subject: `{}`\n", a.subject));
        md.push_str(&format!("- Year level: `{}`\n", a.year_level));
        if let Some(max) = a.max_score {
            md.push_str(&format!("- Max score: **{}**\n", max));
        }
        md.push_str(&format!("- Due: `{}`\n", a.due_at.as_deref().unwrap_or("")));

        if !a.attachments.is_empty() {
            md.push_str("\n## Attachments\n\n");
            for att in &a.attachments {
                md.push_str(&format!("- `{}`\n", att));
            }
        }

        md.push_str("\n## Student-facing instructions\n\n");
        md.push_str(assignment_printable_md(a).trim_end());
        md.push('\n');

        md.push_str("\n## Rubric / Marking guide\n\n");
        if let Some(rubric) = a
            .teacher_rubric_md
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            md.push_str(rubric.trim_end());
            md.push('\n');
        } else {
            md.push_str("_No rubric provided in the pack markdown._\n");
        }

        match fs::write(&out_path, md) {
            Ok(_) => {
                report.written += 1;
                report.outputs.push(out_path);
            }
            Err(err) => {
                report.failed += 1;
                report
                    .errors
                    .push(format!("{}: could not write: {err}", out_path.display()));
            }
        }
    }

    Ok(report)
}

pub fn transcribe_completed_submissions_to_marking_md(
    base: &Path,
    pack: Option<&HomeworkPack>,
) -> io::Result<MarkingTranscribeReport> {
    let completed_dir = base.join("homework").join("completed");
    let marking_dir = homework_marking_dir(base);
    fs::create_dir_all(&completed_dir)?;
    fs::create_dir_all(&marking_dir)?;

    let mut report = MarkingTranscribeReport::default();

    let entries = match fs::read_dir(&completed_dir) {
        Ok(v) => v,
        Err(err) => {
            report.failed += 1;
            report.errors.push(format!(
                "Could not read completed dir {}: {err}",
                completed_dir.display()
            ));
            return Ok(report);
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(OsStr::to_str)
            .map(|e| e.eq_ignore_ascii_case("json"))
            .unwrap_or(false)
            == false
        {
            continue;
        }
        let fname = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !fname.to_ascii_lowercase().starts_with("submission_") {
            continue;
        }

        report.processed += 1;

        let raw = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(err) => {
                report.failed += 1;
                report
                    .errors
                    .push(format!("{}: could not read: {err}", path.display()));
                continue;
            }
        };
        let sub: HomeworkSubmission = match serde_json::from_str(&raw) {
            Ok(s) => s,
            Err(err) => {
                report.failed += 1;
                report
                    .errors
                    .push(format!("{}: JSON parse error: {err}", path.display()));
                continue;
            }
        };

        let assignment = find_assignment_for_submission(pack, &sub);
        let md = submission_to_marking_markdown(&sub, assignment);

        let stem = safe_stem(&path);
        let out_name = format!("marking_{stem}.md");
        let out_path = marking_dir.join(out_name);
        if out_path.exists() {
            report.skipped += 1;
            continue;
        }

        match fs::write(&out_path, md) {
            Ok(_) => {
                report.written += 1;
                report.outputs.push(out_path);
            }
            Err(err) => {
                report.failed += 1;
                report.errors.push(format!(
                    "{}: could not write markdown: {err}",
                    path.display()
                ));
            }
        }
    }

    Ok(report)
}
