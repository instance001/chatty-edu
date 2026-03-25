use crate::homework_pack::{load_completed_submissions, LoadedSubmission};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RevisionSource {
    pub revision_key: String,
    pub assignment_id: String,
    pub assignment_title: String,
    pub subject: String,
    pub year_level: String,
    pub submitted_at: String,
    pub answers_text: String,
    pub ai_score: Option<i32>,
    pub ai_feedback: Option<String>,
    pub instructions_md: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RevisionProgress {
    pub revision_key: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "default_revision_confidence")]
    pub confidence: i32,
    #[serde(default)]
    pub review_count: u32,
    #[serde(default)]
    pub last_reviewed_at: String,
}

fn default_revision_confidence() -> i32 {
    50
}

pub fn revision_dir(base: &Path) -> PathBuf {
    base.join("revision")
}

pub fn revision_notes_dir(base: &Path) -> PathBuf {
    revision_dir(base).join("notes")
}

pub fn revision_past_papers_dir(base: &Path) -> PathBuf {
    revision_dir(base).join("past_papers")
}

pub fn ensure_revision_dirs(base: &Path) -> io::Result<()> {
    fs::create_dir_all(revision_dir(base))?;
    fs::create_dir_all(revision_notes_dir(base))?;
    fs::create_dir_all(revision_past_papers_dir(base))?;
    Ok(())
}

pub fn load_revision_sources(
    base: &Path,
    preferred_student_id: Option<&str>,
) -> io::Result<Vec<RevisionSource>> {
    ensure_revision_dirs(base)?;
    let mut loaded = load_completed_submissions(base)?;

    if let Some(student_id) = preferred_student_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        let filtered: Vec<LoadedSubmission> = loaded
            .iter()
            .filter(|record| record.submission.student_id == student_id)
            .cloned()
            .collect();
        if !filtered.is_empty() {
            loaded = filtered;
        }
    }

    let mut out = Vec::new();
    for record in loaded {
        let submission = record.submission;
        let revision_key = revision_key_for_submission(&submission.student_id, &record.path);
        let assignment_title = submission
            .assignment_title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| submission.assignment_id.clone());
        let subject = submission
            .assignment_subject
            .clone()
            .filter(|subject| !subject.trim().is_empty())
            .unwrap_or_else(|| "General".to_string());
        let year_level = submission
            .assignment_year_level
            .clone()
            .filter(|year| !year.trim().is_empty())
            .unwrap_or_else(|| "-".to_string());
        let instructions_md = submission
            .assignment_instructions_md
            .clone()
            .filter(|text| !text.trim().is_empty());

        out.push(RevisionSource {
            revision_key,
            assignment_id: submission.assignment_id.clone(),
            assignment_title,
            subject,
            year_level,
            submitted_at: submission.submitted_at.clone(),
            answers_text: submission.answers_text.clone().unwrap_or_default(),
            ai_score: submission
                .ai_premark
                .as_ref()
                .and_then(|premark| premark.score),
            ai_feedback: submission
                .ai_premark
                .as_ref()
                .and_then(|premark| premark.feedback.clone())
                .filter(|text| !text.trim().is_empty()),
            instructions_md,
        });
    }

    out.sort_by(|a, b| {
        revision_priority(b)
            .cmp(&revision_priority(a))
            .then_with(|| b.submitted_at.cmp(&a.submitted_at))
            .then_with(|| a.assignment_title.cmp(&b.assignment_title))
    });
    Ok(out)
}

pub fn load_revision_progress(base: &Path) -> io::Result<Vec<RevisionProgress>> {
    ensure_revision_dirs(base)?;
    let dir = revision_notes_dir(base);
    let mut out = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().map(|ext| ext != "json").unwrap_or(true) {
            continue;
        }

        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(_) => continue,
        };
        if let Ok(progress) = serde_json::from_str::<RevisionProgress>(&contents) {
            out.push(progress);
        }
    }

    Ok(out)
}

pub fn save_revision_progress(base: &Path, progress: &RevisionProgress) -> io::Result<PathBuf> {
    ensure_revision_dirs(base)?;
    let path = revision_notes_dir(base).join(format!(
        "revision_note_{}.json",
        sanitize_filename_component(&progress.revision_key)
    ));
    let json = serde_json::to_string_pretty(progress)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load_past_papers(base: &Path) -> io::Result<Vec<PathBuf>> {
    ensure_revision_dirs(base)?;
    let dir = revision_past_papers_dir(base);
    let mut out = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            out.push(path);
        }
    }

    out.sort();
    Ok(out)
}

pub fn import_past_paper(base: &Path, src: &Path) -> io::Result<PathBuf> {
    ensure_revision_dirs(base)?;
    let dir = revision_past_papers_dir(base);
    let file_name = src
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing file name"))?;
    let mut dest = dir.join(file_name);

    if dest.exists() {
        let stem = src
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("past_paper");
        let ext = src.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        let mut counter = 1usize;
        loop {
            let candidate = if ext.is_empty() {
                dir.join(format!("{stem}_{counter}"))
            } else {
                dir.join(format!("{stem}_{counter}.{ext}"))
            };
            if !candidate.exists() {
                dest = candidate;
                break;
            }
            counter += 1;
        }
    }

    fs::copy(src, &dest)?;
    Ok(dest)
}

pub fn build_revision_pack_markdown(
    base: &Path,
    sources: &[RevisionSource],
) -> io::Result<PathBuf> {
    ensure_revision_dirs(base)?;
    let stamp = Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let path = revision_dir(base).join(format!("revision_pack_{stamp}.md"));

    let mut out = String::new();
    out.push_str("# Revision Pack\n\n");
    out.push_str("Generated from completed homework submissions.\n\n");

    if sources.is_empty() {
        out.push_str(
            "_No completed homework submissions were available when this pack was created._\n",
        );
    } else {
        out.push_str("## Priority review items\n\n");
        for source in sources.iter().take(12) {
            out.push_str(&format!(
                "### {} ({})\n\n",
                source.assignment_title.as_str(),
                source.assignment_id.as_str()
            ));
            out.push_str(&format!("- Subject: {}\n", source.subject.as_str()));
            out.push_str(&format!("- Year level: {}\n", source.year_level.as_str()));
            out.push_str(&format!("- Submitted: {}\n", source.submitted_at.as_str()));
            if let Some(score) = source.ai_score {
                out.push_str(&format!("- AI score: {}\n", score));
            }
            if let Some(feedback) = source.ai_feedback.as_deref() {
                out.push_str(&format!("- Feedback focus: {}\n", feedback.trim()));
            }
            out.push('\n');
            if let Some(instructions) = source.instructions_md.as_deref() {
                out.push_str("#### Original assignment snapshot\n\n");
                out.push_str(instructions.trim());
                out.push_str("\n\n");
            }
            if !source.answers_text.trim().is_empty() {
                out.push_str("#### Student submission snapshot\n\n");
                out.push_str(source.answers_text.trim());
                out.push_str("\n\n");
            }
        }
    }

    fs::write(&path, out)?;
    Ok(path)
}

pub fn revision_priority(source: &RevisionSource) -> i32 {
    let score_weight = source
        .ai_score
        .map(|score| 100 - score)
        .unwrap_or(25)
        .max(0);
    let feedback_weight = if source
        .ai_feedback
        .as_deref()
        .is_some_and(|text| !text.trim().is_empty())
    {
        15
    } else {
        0
    };
    score_weight + feedback_weight
}

fn revision_key_for_submission(student_id: &str, path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("revision");
    format!(
        "{}__{}",
        sanitize_filename_component(student_id),
        sanitize_filename_component(stem)
    )
}

fn sanitize_filename_component(text: &str) -> String {
    text.trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
