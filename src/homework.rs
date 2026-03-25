use std::path::Path;

use crate::homework_pack::{load_submission_summaries, SubmissionSummary};

/// Print a simple table of completed submission entries.
pub fn print_submission_table(items: &[SubmissionSummary]) {
    if items.is_empty() {
        println!("\nNo completed submissions found yet.\n");
        return;
    }

    println!();
    println!(
        "{:<14} | {:<10} | {:<12} | {:<5} | {:<20} | {}",
        "Student", "Student ID", "Assignment", "Score", "Submitted", "AI feedback"
    );
    println!("{}", "-".repeat(100));

    for s in items {
        let score_str = s
            .score
            .or(s.ai_score)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string());
        let feedback = s.ai_feedback.as_deref().unwrap_or("-").replace('\n', " ");
        println!(
            "{:<14} | {:<10} | {:<12} | {:<5} | {:<20} | {}",
            truncate_for_table(&s.student_name, 14),
            truncate_for_table(&s.student_id, 10),
            truncate_for_table(&s.assignment_id, 12),
            score_str,
            truncate_for_table(&s.submitted_at, 20),
            truncate_for_table(&feedback, 40),
        );
    }

    println!();
}

fn truncate_for_table(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let mut out = String::new();
        for (i, ch) in s.chars().enumerate() {
            if i >= max_len.saturating_sub(3) {
                out.push_str("...");
                break;
            }
            out.push(ch);
        }
        out
    }
}

/// Top-level helper the rest of the app can call from teacher mode.
pub fn show_homework_dashboard(base_path: &Path) {
    match load_submission_summaries(base_path) {
        Ok(list) => {
            println!("\nCompleted submissions overview:");
            print_submission_table(&list);
        }
        Err(e) => {
            eprintln!("\n[ERROR] Could not load completed submissions: {}\n", e);
        }
    }
}
