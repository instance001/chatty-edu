use chrono::Utc;
use deunicode::deunicode;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ColdLogEntry {
    pub session_id: String,
    pub timestamp: String,
    pub channel: String,
    pub speaker: String,
    pub text: String,
    #[serde(default)]
    pub note: Option<String>,
}

enum BookkeeperCmd {
    Append(ColdLogEntry),
    Search {
        query: String,
        respond_to: Sender<Vec<ColdLogEntry>>,
    },
    Shutdown,
}

pub struct EduBookkeeperHandle {
    tx: Sender<BookkeeperCmd>,
    session_id: String,
    join: Option<JoinHandle<()>>,
}

impl EduBookkeeperHandle {
    pub fn start(base: &Path) -> Option<Self> {
        fs::create_dir_all(bookkeeper_dir(base)).ok()?;

        let cold_log_path = cold_log_path(base);
        let memory_jogger_path = memory_jogger_path(base);
        let session_id = format!("edu-{}", Utc::now().format("%Y%m%dT%H%M%SZ"));
        let thread_session_id = session_id.clone();
        let (tx, rx) = mpsc::channel::<BookkeeperCmd>();

        let join = thread::Builder::new()
            .name("chatty_edu_bookkeeper".to_string())
            .spawn(move || {
                let session_start = ColdLogEntry {
                    session_id: thread_session_id.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    channel: "session".to_string(),
                    speaker: "System".to_string(),
                    text: "Session started".to_string(),
                    note: None,
                };
                append_cold_log(&cold_log_path, &session_start);

                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        BookkeeperCmd::Append(entry) => {
                            append_cold_log(&cold_log_path, &entry);
                        }
                        BookkeeperCmd::Search { query, respond_to } => {
                            let _ = respond_to.send(search_cold_logs(&cold_log_path, &query));
                        }
                        BookkeeperCmd::Shutdown => {
                            let previous =
                                fs::read_to_string(&memory_jogger_path).unwrap_or_default();
                            let session_entries =
                                read_session_entries(&cold_log_path, &thread_session_id);
                            let session_summary = build_session_summary(&session_entries);
                            let updated = merge_memory_jogger(&previous, &session_summary);
                            if !updated.trim().is_empty() {
                                let _ = fs::write(&memory_jogger_path, updated.as_bytes());
                            }

                            if !session_summary.trim().is_empty() {
                                let summary_entry = ColdLogEntry {
                                    session_id: thread_session_id.clone(),
                                    timestamp: Utc::now().to_rfc3339(),
                                    channel: "session".to_string(),
                                    speaker: "System".to_string(),
                                    text: "Memory jogger updated".to_string(),
                                    note: Some(session_summary.replace('\n', " | ")),
                                };
                                append_cold_log(&cold_log_path, &summary_entry);
                            }

                            let session_end = ColdLogEntry {
                                session_id: thread_session_id.clone(),
                                timestamp: Utc::now().to_rfc3339(),
                                channel: "session".to_string(),
                                speaker: "System".to_string(),
                                text: "Session closed".to_string(),
                                note: None,
                            };
                            append_cold_log(&cold_log_path, &session_end);
                            break;
                        }
                    }
                }
            })
            .ok()?;

        Some(Self {
            tx,
            session_id,
            join: Some(join),
        })
    }

    pub fn append_chat_entry(&self, speaker: &str, text: &str, note: Option<String>) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }

        let sanitized_text = sanitize_for_bookkeeper_storage(trimmed);
        if sanitized_text.is_empty() {
            return;
        }

        let entry = ColdLogEntry {
            session_id: self.session_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            channel: "chat".to_string(),
            speaker: speaker.trim().to_string(),
            text: truncate(&sanitized_text, 2000),
            note: note
                .map(|value| sanitize_for_bookkeeper_storage(&value))
                .filter(|value| !value.is_empty()),
        };
        let _ = self.tx.send(BookkeeperCmd::Append(entry));
    }

    pub fn search(&self, query: &str) -> Vec<ColdLogEntry> {
        let (send, recv) = mpsc::channel();
        if self
            .tx
            .send(BookkeeperCmd::Search {
                query: query.trim().to_string(),
                respond_to: send,
            })
            .is_err()
        {
            return Vec::new();
        }

        recv.recv_timeout(Duration::from_secs(2))
            .unwrap_or_default()
    }

    pub fn shutdown_silently(mut self) {
        let _ = self.tx.send(BookkeeperCmd::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub fn bookkeeper_dir(base: &Path) -> PathBuf {
    base.join("config").join("bookkeeper")
}

pub fn cold_log_path(base: &Path) -> PathBuf {
    bookkeeper_dir(base).join("cold_log.jsonl")
}

pub fn memory_jogger_path(base: &Path) -> PathBuf {
    bookkeeper_dir(base).join("memory_jogger.txt")
}

pub fn load_memory_jogger(base: &Path) -> String {
    fs::read_to_string(memory_jogger_path(base))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn append_cold_log(path: &Path, entry: &ColdLogEntry) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let Ok(line) = serde_json::to_string(entry) else {
        return;
    };

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{line}");
}

fn search_cold_logs(path: &Path, query: &str) -> Vec<ColdLogEntry> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);
    let needle = query.trim().to_ascii_lowercase();
    let mut matches = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(entry) = serde_json::from_str::<ColdLogEntry>(&line) else {
            continue;
        };

        if needle.is_empty() {
            matches.push(entry);
            continue;
        }

        let haystack = format!(
            "{} {} {} {} {}",
            entry.session_id,
            entry.timestamp,
            entry.speaker,
            entry.text,
            entry.note.clone().unwrap_or_default()
        )
        .to_ascii_lowercase();

        if haystack.contains(&needle) {
            matches.push(entry);
        }
    }

    matches.reverse();
    matches.truncate(250);
    matches
}

fn read_session_entries(path: &Path, session_id: &str) -> Vec<ColdLogEntry> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);

    reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<ColdLogEntry>(&line).ok())
        .filter(|entry| entry.session_id == session_id)
        .collect()
}

fn merge_memory_jogger(previous: &str, session_summary: &str) -> String {
    let mut blocks = Vec::new();
    let current = session_summary.trim();
    if !current.is_empty() {
        blocks.push(current.to_string());
    }

    for block in split_summary_blocks(previous) {
        if !blocks.iter().any(|existing| existing == &block) {
            blocks.push(block);
        }
    }

    blocks.truncate(3);
    blocks.join("\n\n")
}

fn split_summary_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                blocks.push(current.join("\n"));
                current.clear();
            }
            continue;
        }
        current.push(trimmed.to_string());
    }

    if !current.is_empty() {
        blocks.push(current.join("\n"));
    }

    blocks
}

fn build_session_summary(session_entries: &[ColdLogEntry]) -> String {
    let chat_entries = session_entries
        .iter()
        .filter(|entry| entry.channel == "chat" && !entry.text.trim().is_empty())
        .collect::<Vec<_>>();
    if chat_entries.is_empty() {
        return String::new();
    }

    let session_day = session_entries
        .first()
        .and_then(|entry| entry.timestamp.split('T').next())
        .map(str::to_string)
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());

    let topic = summarize_session_topic(session_entries, &chat_entries);
    let key_questions = summarize_key_questions(&chat_entries);
    let difficulty = summarize_difficulty(&chat_entries);
    let last_exchange = summarize_last_exchange(&chat_entries);

    [
        format!("- {session_day}: {topic}"),
        format!("- Key questions asked: {key_questions}"),
        format!("- Areas of difficulty: {difficulty}"),
        format!("- Last session ended on: {last_exchange}"),
    ]
    .join("\n")
}

fn summarize_session_topic(
    session_entries: &[ColdLogEntry],
    chat_entries: &[&ColdLogEntry],
) -> String {
    if let Some(note) = session_entries
        .iter()
        .filter_map(|entry| entry.note.as_deref())
        .map(str::trim)
        .find(|note| note.starts_with("Homework context:"))
    {
        return note
            .trim_start_matches("Homework context:")
            .trim()
            .to_string();
    }

    let user_messages = chat_entries
        .iter()
        .filter(|entry| entry.speaker.eq_ignore_ascii_case("you"))
        .map(|entry| entry.text.as_str())
        .collect::<Vec<_>>();
    let topics = top_topic_tokens(&user_messages);
    if !topics.is_empty() {
        return format!("worked on {}", join_readable(&topics));
    }

    "worked on general study support".to_string()
}

fn summarize_key_questions(chat_entries: &[&ColdLogEntry]) -> String {
    let mut questions = Vec::new();
    for entry in chat_entries {
        if !entry.speaker.eq_ignore_ascii_case("you") {
            continue;
        }
        let text = entry.text.trim();
        if text.is_empty() {
            continue;
        }
        let lower = text.to_ascii_lowercase();
        let looks_like_question = text.ends_with('?')
            || lower.contains("help")
            || lower.contains("question")
            || lower.contains("how")
            || lower.contains("what")
            || lower.contains("why")
            || lower.contains("explain");
        if looks_like_question {
            let snippet = collapse_sentence(text);
            if !questions.iter().any(|existing| existing == &snippet) {
                questions.push(snippet);
            }
        }
    }

    if questions.is_empty() {
        "general study questions".to_string()
    } else {
        join_readable(&questions.into_iter().take(2).collect::<Vec<_>>())
    }
}

fn summarize_difficulty(chat_entries: &[&ColdLogEntry]) -> String {
    for entry in chat_entries {
        if !entry.speaker.eq_ignore_ascii_case("you") {
            continue;
        }

        let lower = entry.text.to_ascii_lowercase();
        if [
            "don't understand",
            "do not understand",
            "confused",
            "stuck",
            "hard",
            "difficult",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
        {
            return collapse_sentence(entry.text.trim());
        }
        if ["help", "hint", "question", "how do i", "can you explain"]
            .iter()
            .any(|needle| lower.contains(needle))
        {
            return format!("needed help with {}", collapse_sentence(entry.text.trim()));
        }
    }

    "No clear sticking point was stated.".to_string()
}

fn summarize_last_exchange(chat_entries: &[&ColdLogEntry]) -> String {
    let meaningful = chat_entries
        .iter()
        .filter(|entry| !entry.text.trim().is_empty() && entry.text.trim() != "...")
        .copied()
        .collect::<Vec<_>>();

    if let Some(last) = meaningful.last() {
        if last.speaker.eq_ignore_ascii_case("chatty") {
            if let Some(user_entry) = meaningful
                .iter()
                .rev()
                .skip(1)
                .find(|entry| entry.speaker.eq_ignore_ascii_case("you"))
            {
                return format!(
                    "you asked \"{}\" and Chatty replied \"{}\"",
                    collapse_sentence(user_entry.text.trim()),
                    collapse_sentence(last.text.trim())
                );
            }
        }

        return format!(
            "{} said \"{}\"",
            last.speaker,
            collapse_sentence(last.text.trim())
        );
    }

    "recent study chat.".to_string()
}

fn top_topic_tokens(messages: &[&str]) -> Vec<String> {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();

    for message in messages {
        for token in tokenize(message) {
            if is_topic_stopword(&token) {
                continue;
            }
            *counts.entry(token).or_insert(0) += 1;
        }
    }

    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.into_iter().take(4).map(|(token, _)| token).collect()
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 3)
        .map(|token| token.to_string())
        .collect()
}

fn is_topic_stopword(token: &str) -> bool {
    matches!(
        token,
        "about"
            | "answer"
            | "are"
            | "asked"
            | "chatty"
            | "could"
            | "does"
            | "find"
            | "from"
            | "help"
            | "homework"
            | "just"
            | "into"
            | "like"
            | "need"
            | "please"
            | "question"
            | "revision"
            | "school"
            | "show"
            | "solve"
            | "some"
            | "thanks"
            | "that"
            | "them"
            | "this"
            | "use"
            | "what"
            | "with"
            | "work"
            | "would"
            | "you"
    )
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in text.chars().take(max_chars) {
        out.push(ch);
    }
    if text.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn collapse_sentence(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn sanitize_for_bookkeeper_storage(text: &str) -> String {
    let without_templates = strip_prompt_template_markers(text);
    let without_markdown = strip_markdown_formatting(&without_templates);
    ascii_plain_text(&without_markdown)
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

fn strip_markdown_formatting(text: &str) -> String {
    text.replace("```", " ")
        .replace("`", "")
        .replace("**", "")
        .replace("__", "")
        .replace("~~", "")
        .replace("###", " ")
        .replace("##", " ")
        .replace('#', " ")
        .replace('|', " ")
        .replace('>', " ")
}

fn ascii_plain_text(text: &str) -> String {
    let normalized = normalize_common_mojibake(text);
    let mut out = String::new();

    for ch in normalized.chars() {
        match ch {
            '\r' | '\n' | '\t' | '\u{00A0}' => out.push(' '),
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
            '│' | '┃' | '┆' | '┇' | '┊' | '┋' | '¦' | '｜' => out.push(' '),
            '─' | '━' | '═' | '﹘' | '﹣' | '－' => out.push('-'),
            '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' | '╭' | '╮' | '╰' | '╯' | '╳'
            | '╋' | '╂' | '╬' | '╠' | '╣' | '╦' | '╩' => out.push(' '),
            c if c.is_control() => {}
            c if c.is_ascii() => out.push(c),
            other => {
                let transliterated = deunicode(&other.to_string());
                if transliterated.is_empty() {
                    out.push(' ');
                } else {
                    for fallback in transliterated.chars() {
                        if fallback.is_ascii_graphic() || fallback == ' ' {
                            out.push(fallback);
                        }
                    }
                }
            }
        }
    }

    collapse_sentence(&out)
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

fn join_readable(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [one, two] => format!("{one} and {two}"),
        _ => {
            let mut out = items[..items.len() - 1].join(", ");
            out.push_str(", and ");
            out.push_str(&items[items.len() - 1]);
            out
        }
    }
}
