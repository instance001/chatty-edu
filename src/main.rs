use chrono::Utc;
use clap::{Parser, ValueEnum};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

mod chat;
mod ecg_window;
mod gui;
mod homework;
mod homework_markdown;
mod homework_pack;
mod local_model;
mod memory;
mod model_registry;
#[allow(dead_code)]
mod module_bridge;
mod module_host;
mod modules;
mod networking;
mod revision;
mod sandbox;
mod settings;
mod theme;

use chat::{generate_answer, janet_filter};
use homework_markdown::{
    export_student_printables, export_teacher_rubrics, homework_marking_dir, homework_outgoing_dir,
    homework_printables_dir, homework_rubrics_dir, parse_homework_pack_markdown,
    transcribe_completed_submissions_to_marking_md, transcribe_outgoing_packs, PackMdDefaults,
};
use homework_pack::{
    apply_pack_policy, create_pack, create_pack_multi, export_pack_template, find_latest_pack,
    load_pack_from_file, load_submission_summaries, save_submission_with_answers,
    HomeworkAssignment,
};
use model_registry::auto_assign_model_roles;
use settings::{
    default_base_path, ensure_base_folders, load_or_init_settings, save_settings, Settings,
};

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

#[derive(Parser, Debug)]
#[command(
    name = "chatty-edu",
    version,
    about = "Chatty-EDU (local-first, offline)"
)]
struct CliArgs {
    /// Choose GUI (default) or CLI mode
    #[arg(long, value_enum, default_value = "gui")]
    mode: RunMode,
    /// Override data base path (defaults to ./data next to the exe)
    #[arg(long)]
    base_path: Option<PathBuf>,
    /// Path to model file (GGUF). Used by the internal model worker.
    #[arg(long, hide = true)]
    model_path: Option<PathBuf>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum RunMode {
    Gui,
    Cli,
    #[value(name = "model-worker", hide = true)]
    ModelWorker,
}

fn main() {
    let args = CliArgs::parse();
    let base_path = args.base_path.unwrap_or_else(default_base_path);

    if let Err(e) = ensure_base_folders(&base_path) {
        eprintln!(
            "Failed to create base folders at {}: {}",
            base_path.display(),
            e
        );
        return;
    }

    let mut settings = match load_or_init_settings(&base_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to load settings: {}", e);
            return;
        }
    };

    let model_roles = auto_assign_model_roles(&mut settings, &base_path);
    if model_roles.changed {
        let _ = save_settings(&settings, &base_path);
    }

    if matches!(args.mode, RunMode::ModelWorker) {
        let model_path = args
            .model_path
            .unwrap_or_else(|| PathBuf::from(settings.model.path.clone()));
        let code = local_model::run_model_worker(&model_path, settings.model.max_tokens);
        std::process::exit(code);
    }

    println!("Using data path: {}", base_path.display());

    // Apply latest homework pack policy (e.g., games allowed/blocked) if present.
    if let Ok(Some((_pack_path, pack))) = find_latest_pack(&base_path) {
        apply_pack_policy(&mut settings, &pack);
    }

    settings.base_path = base_path.to_string_lossy().to_string();
    settings.mode = match args.mode {
        RunMode::Gui => "gui".to_string(),
        RunMode::Cli => "cli".to_string(),
        RunMode::ModelWorker => "model-worker".to_string(),
    };

    match args.mode {
        RunMode::Gui => {
            if let Err(e) = gui::launch_gui(base_path.clone(), settings.clone()) {
                eprintln!("Failed to start GUI: {}", e);
            }
        }
        RunMode::Cli => {
            run_cli(&mut settings, &base_path);
        }
        RunMode::ModelWorker => unreachable!("model-worker mode exits before launching UI/CLI"),
    }

    if let Err(e) = save_settings(&settings, &base_path) {
        eprintln!("Could not save settings: {}", e);
    }
}

fn run_cli(settings: &mut Settings, base_path: &Path) {
    println!("Chatty-EDU v{} CLI starting up", env!("CARGO_PKG_VERSION"));
    println!("Base path: {}", base_path.display());
    println!("Mode: {}", settings.mode);
    println!("Type 'exit' to quit, 'teacher' for teacher console, 'play' to try game mode.\n");

    loop {
        println!(
            "[Mode: {} | TeacherMode: {}]",
            settings.mode, settings.teacher_mode
        );
        print!("You (or command): ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            println!("Error reading input. Exiting.");
            break;
        }

        let input = input.trim();
        if input.eq_ignore_ascii_case("exit") {
            println!("Goodbye");
            break;
        }

        if input.eq_ignore_ascii_case("teacher") {
            teacher_console(settings, base_path);
            continue;
        }

        if let Some(rest) = input.strip_prefix("submit ") {
            let assignment_id = rest.trim();
            if assignment_id.is_empty() {
                println!("Usage: submit <assignment_id>");
            } else {
                let answers = prompt("Answer text", "My work goes here").unwrap_or_default();
                let attachments_input =
                    prompt("Attachment paths (comma-separated, optional)", "").unwrap_or_default();
                let attachments: Vec<String> = attachments_input
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                match save_submission_with_answers(
                    base_path,
                    settings,
                    assignment_id,
                    &answers,
                    &attachments,
                ) {
                    Ok(path) => println!("Wrote submission to {}", path.display()),
                    Err(e) => println!("Failed to write submission: {}", e),
                }
            }
            continue;
        }

        if input.to_lowercase().starts_with("play") {
            handle_play_request(settings);
            continue;
        }

        if input.is_empty() {
            continue;
        }

        let raw_answer = generate_answer(settings, input);
        let safe_answer = janet_filter(&settings.janet, &raw_answer, input);

        println!("Chatty: {safe_answer}\n");
    }
}

fn handle_play_request(settings: &Settings) {
    if !settings.game.enabled {
        println!("\n[Play] Games are currently DISABLED in settings.\n");
        return;
    }

    if settings.teacher_mode == "class" && !settings.game.games_in_class_allowed {
        println!("\n[Play] Games are not allowed in CLASS mode.\n");
        return;
    }

    println!("\n[Play] Game mode is not implemented yet, but the hook is working.\n");
}

fn teacher_console(settings: &mut Settings, base_path: &Path) {
    println!("\nEnter teacher PIN (default 0000):");
    println!("Type 'forgot' to answer the secret question.");
    print!("PIN: ");
    io::stdout().flush().unwrap();

    let mut pin_input = String::new();
    if let Err(e) = io::stdin().read_line(&mut pin_input) {
        println!("Failed to read PIN: {}", e);
        return;
    }

    let pin_input = pin_input.trim();
    if pin_input.eq_ignore_ascii_case("forgot") {
        println!("Secret question: {}", settings.teacher_secret_question);
        print!("Answer: ");
        io::stdout().flush().unwrap();
        let mut answer = String::new();
        if let Err(e) = io::stdin().read_line(&mut answer) {
            println!("Failed to read answer: {}", e);
            return;
        }
        if answer.trim() != settings.teacher_secret_answer {
            println!("Incorrect answer.\n");
            return;
        }
        println!("Unlocked with secret question.\n");
    } else if settings.teacher_pin != pin_input {
        println!("Incorrect PIN.\n");
        return;
    }

    println!("\nTeacher console\n");

    loop {
        println!("Current teacher mode: {}", settings.teacher_mode);
        println!("Games enabled: {}", settings.game.enabled);
        println!(
            "Games allowed in class: {}",
            settings.game.games_in_class_allowed
        );
        println!("Base path: {}", base_path.display());
        println!(
            "Homework (assigned): {}",
            base_path.join("homework").join("assigned").display()
        );
        println!(
            "Homework (completed): {}",
            base_path.join("homework").join("completed").display()
        );
        println!(
            "Homework (outgoing): {}",
            base_path.join("homework").join("outgoing").display()
        );
        println!(
            "Homework (marking): {}",
            base_path.join("homework").join("marking").display()
        );
        println!(
            "Homework (printables): {}",
            base_path.join("homework").join("printables").display()
        );
        println!(
            "Homework (rubrics): {}",
            base_path.join("homework").join("rubrics").display()
        );
        println!("Commands:");
        println!("  mode class");
        println!("  mode free");
        println!("  games on");
        println!("  games off");
        println!("  allow_games_in_class");
        println!("  forbid_games_in_class");
        println!("  show_completed    (show table of completed homework)");
        println!("  homework table    (alias for show_completed)");
        println!("  export_pack_template  (writes a homework_pack template to assigned/)");
        println!("  create_pack           (interactive pack builder, single assignment)");
        println!("  create_pack_multi     (interactive pack builder, multi assignment)");
        println!("  import_submissions    (summarize submission_*.json in completed/)");
        println!(
            "  import_pack <path>    (import .json into assigned/ or .md into outgoing/ and transcribe)"
        );
        println!("  generate_pack_md      (use local model to draft a pack .md into outgoing/)");
        println!("  transcribe_outgoing   (convert outgoing/*.md into assigned/*.json)");
        println!("  convert_submissions_to_md   (export completed submissions into marking/*.md)");
        println!("  export_printables     (export student printables into printables/*.md)");
        println!("  export_rubrics        (export teacher rubrics into rubrics/*.md)");
        println!("  set_pin               (change teacher PIN; confirm twice)");
        println!("  set_secret            (change secret question + answer)");
        println!("  back");

        print!("teacher> ");
        io::stdout().flush().ok();

        let mut input = String::new();
        if let Err(e) = io::stdin().read_line(&mut input) {
            println!("Input error ({}), exiting teacher console.", e);
            break;
        }

        let cmd = input.trim();

        match cmd {
            "mode class" => {
                settings.teacher_mode = "class".to_string();
                println!("Teacher mode set to CLASS.");
            }
            "mode free" => {
                settings.teacher_mode = "free_time".to_string();
                println!("Teacher mode set to FREE TIME.");
            }
            "games on" => {
                settings.game.enabled = true;
                println!("Games ENABLED.");
            }
            "games off" => {
                settings.game.enabled = false;
                println!("Games DISABLED.");
            }
            "allow_games_in_class" => {
                settings.game.games_in_class_allowed = true;
                println!("Games allowed in CLASS mode.");
            }
            "forbid_games_in_class" => {
                settings.game.games_in_class_allowed = false;
                println!("Games forbidden in CLASS mode.");
            }
            "show_completed" | "homework table" => {
                homework::show_homework_dashboard(base_path);
            }
            "export_pack_template" => {
                match export_pack_template(base_path, "school", &settings.student.class_id) {
                    Ok(path) => println!("Pack template written to {}", path.display()),
                    Err(e) => println!("Failed to write template: {}", e),
                }
            }
            "create_pack" => match create_pack_interactive(base_path) {
                Ok(path) => println!("Pack written to {}", path.display()),
                Err(e) => println!("Failed to write pack: {}", e),
            },
            "create_pack_multi" => match create_pack_multi_interactive(base_path) {
                Ok(path) => println!("Pack written to {}", path.display()),
                Err(e) => println!("Failed to write pack: {}", e),
            },
            "import_submissions" => match load_submission_summaries(base_path) {
                Ok(list) => {
                    if list.is_empty() {
                        println!("No submission_*.json files found in completed/.");
                    } else {
                        println!("Submissions:");
                        for s in list {
                            let score = s
                                .score
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".to_string());
                            println!(
                                "  {} by {} ({}) score: {}",
                                s.assignment_id, s.student_name, s.student_id, score
                            );
                        }
                    }
                }
                Err(e) => println!("Failed to read submissions: {}", e),
            },
            "generate_pack_md" => {
                let request = match prompt(
                    "Pack request (describe year/subject/topics/number of questions)",
                    "Year 7 Math fractions: 10 questions, include 1 word problem",
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        println!("Failed to read request: {}", e);
                        continue;
                    }
                };
                if request.trim().is_empty() {
                    println!("Request cannot be empty.");
                    continue;
                }

                let existing_pack = find_latest_pack(base_path).ok().flatten().map(|(_, p)| p);
                let defaults = PackMdDefaults {
                    version: "1.0".to_string(),
                    school_id: existing_pack
                        .as_ref()
                        .map(|p| p.school_id.clone())
                        .unwrap_or_else(|| "school".to_string()),
                    class_id: if settings.student.class_id.trim().is_empty() {
                        existing_pack
                            .as_ref()
                            .map(|p| p.class_id.clone())
                            .unwrap_or_else(|| "class".to_string())
                    } else {
                        settings.student.class_id.trim().to_string()
                    },
                };

                let prompt = format!(
                    "{capsule}\nDefaults:\nversion: 1.0\nschool_id: {school}\nclass_id: {class}\ncreated_at: {now}\n\nTeacher request: {req}",
                    capsule = TEACHER_PACK_CAPSULE,
                    school = defaults.school_id.as_str(),
                    class = defaults.class_id.as_str(),
                    now = Utc::now().to_rfc3339(),
                    req = request.trim()
                );

                let mut model_cfg = settings.model.clone();
                model_cfg.max_tokens = model_cfg.max_tokens.max(1024);
                let raw_md = match local_model::chat_completion(&model_cfg, &prompt) {
                    Ok(md) => md,
                    Err(err) => {
                        println!("AI generation failed: {err}");
                        continue;
                    }
                };
                let md = raw_md.trim().to_string();
                if md.is_empty() {
                    println!("Model returned an empty pack draft.");
                    continue;
                }

                let outgoing = homework_outgoing_dir(base_path);
                if let Err(e) = std::fs::create_dir_all(&outgoing) {
                    println!(
                        "Failed to create outgoing dir {}: {}",
                        outgoing.display(),
                        e
                    );
                    continue;
                }

                let ts = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
                let class_tag = sanitize_filename_component(&defaults.class_id);
                let base_name = if class_tag.trim().is_empty() {
                    format!("homework_pack_{ts}")
                } else {
                    format!("homework_pack_{class_tag}_{ts}")
                };
                let mut out_path = outgoing.join(format!("{base_name}.md"));
                let mut n = 1usize;
                while out_path.exists() {
                    out_path = outgoing.join(format!("{base_name}_{n}.md"));
                    n += 1;
                }

                match std::fs::write(&out_path, format!("{}\n", md.trim_end())) {
                    Ok(_) => {
                        println!("Wrote pack draft to {}", out_path.display());
                        if let Err(err) = parse_homework_pack_markdown(&md, &defaults) {
                            println!("Warning: draft did not parse cleanly: {}", err);
                            println!("Edit the .md, then run transcribe_outgoing.");
                        }
                    }
                    Err(e) => println!("Failed to write outgoing pack: {}", e),
                }
            }
            "transcribe_outgoing" => {
                let existing_pack = find_latest_pack(base_path).ok().flatten().map(|(_, p)| p);
                let defaults = PackMdDefaults {
                    version: "1.0".to_string(),
                    school_id: existing_pack
                        .as_ref()
                        .map(|p| p.school_id.clone())
                        .unwrap_or_else(|| "school".to_string()),
                    class_id: if settings.student.class_id.trim().is_empty() {
                        existing_pack
                            .as_ref()
                            .map(|p| p.class_id.clone())
                            .unwrap_or_else(|| "class".to_string())
                    } else {
                        settings.student.class_id.trim().to_string()
                    },
                };

                let outgoing = homework_outgoing_dir(base_path);
                println!("Outgoing folder: {}", outgoing.display());
                match transcribe_outgoing_packs(base_path, &defaults) {
                    Ok(report) => {
                        println!(
                            "Outgoing -> JSON: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        );
                        for p in report.outputs.iter().take(10) {
                            println!("  wrote {}", p.display());
                        }
                        if report.outputs.len() > 10 {
                            println!("  ... plus {}", report.outputs.len() - 10);
                        }
                        if let Some(first) = report.errors.first() {
                            println!("First error: {}", first);
                        }

                        if let Ok(Some((_path, pack))) = find_latest_pack(base_path) {
                            apply_pack_policy(settings, &pack);
                            if let Err(e) = save_settings(settings, base_path) {
                                println!("Wrote packs but failed to save settings: {}", e);
                            }
                        }
                    }
                    Err(e) => println!("Transcribe failed: {}", e),
                }
            }
            "convert_submissions_to_md" => {
                let pack = find_latest_pack(base_path).ok().flatten().map(|(_, p)| p);
                let marking = homework_marking_dir(base_path);
                println!("Marking folder: {}", marking.display());
                match transcribe_completed_submissions_to_marking_md(base_path, pack.as_ref()) {
                    Ok(report) => {
                        println!(
                            "Submissions -> marking .md: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        );
                        for p in report.outputs.iter().take(10) {
                            println!("  wrote {}", p.display());
                        }
                        if report.outputs.len() > 10 {
                            println!("  ... plus {}", report.outputs.len() - 10);
                        }
                        if let Some(first) = report.errors.first() {
                            println!("First error: {}", first);
                        }
                    }
                    Err(e) => println!("Conversion failed: {}", e),
                }
            }
            "export_printables" => {
                let Some((_path, pack)) = find_latest_pack(base_path).ok().flatten() else {
                    println!(
                        "No pack found in assigned/. Import/transcribe a pack first (then export)."
                    );
                    continue;
                };
                let dir = homework_printables_dir(base_path);
                println!("Printables folder: {}", dir.display());
                match export_student_printables(base_path, &pack) {
                    Ok(report) => {
                        println!(
                            "Student printables: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        );
                        for p in report.outputs.iter().take(10) {
                            println!("  wrote {}", p.display());
                        }
                        if report.outputs.len() > 10 {
                            println!("  ... plus {}", report.outputs.len() - 10);
                        }
                        if let Some(first) = report.errors.first() {
                            println!("First error: {}", first);
                        }
                    }
                    Err(e) => println!("Export failed: {}", e),
                }
            }
            "export_rubrics" => {
                let Some((_path, pack)) = find_latest_pack(base_path).ok().flatten() else {
                    println!(
                        "No pack found in assigned/. Import/transcribe a pack first (then export)."
                    );
                    continue;
                };
                let dir = homework_rubrics_dir(base_path);
                println!("Rubrics folder: {}", dir.display());
                match export_teacher_rubrics(base_path, &pack) {
                    Ok(report) => {
                        println!(
                            "Teacher rubrics: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        );
                        for p in report.outputs.iter().take(10) {
                            println!("  wrote {}", p.display());
                        }
                        if report.outputs.len() > 10 {
                            println!("  ... plus {}", report.outputs.len() - 10);
                        }
                        if let Some(first) = report.errors.first() {
                            println!("First error: {}", first);
                        }
                    }
                    Err(e) => println!("Export failed: {}", e),
                }
            }
            _ if cmd.starts_with("import_pack ") => {
                let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
                if parts.len() < 2 {
                    println!("Usage: import_pack <path_to_pack.json|path_to_pack.md>");
                } else {
                    let src = PathBuf::from(parts[1].trim());
                    if !src.exists() {
                        println!("File not found: {}", src.display());
                    } else {
                        let ext = src
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase();

                        if ext == "md" {
                            let outgoing_dir = homework_outgoing_dir(base_path);
                            if let Err(e) = std::fs::create_dir_all(&outgoing_dir) {
                                println!("Failed to create outgoing dir: {}", e);
                                continue;
                            }
                            let file_name = src
                                .file_name()
                                .unwrap_or_else(|| std::ffi::OsStr::new("homework_pack_import.md"));

                            let mut dest = outgoing_dir.join(file_name);
                            let mut n = 1usize;
                            while dest.exists() {
                                let stem = dest
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("homework_pack_import");
                                dest = outgoing_dir.join(format!("{stem}_{n}.md"));
                                n += 1;
                            }

                            match std::fs::copy(&src, &dest) {
                                Ok(_) => {
                                    println!("Imported markdown pack to {}", dest.display());

                                    let existing_pack =
                                        find_latest_pack(base_path).ok().flatten().map(|(_, p)| p);
                                    let defaults = PackMdDefaults {
                                        version: "1.0".to_string(),
                                        school_id: existing_pack
                                            .as_ref()
                                            .map(|p| p.school_id.clone())
                                            .unwrap_or_else(|| "school".to_string()),
                                        class_id: if settings.student.class_id.trim().is_empty() {
                                            existing_pack
                                                .as_ref()
                                                .map(|p| p.class_id.clone())
                                                .unwrap_or_else(|| "class".to_string())
                                        } else {
                                            settings.student.class_id.trim().to_string()
                                        },
                                    };

                                    match transcribe_outgoing_packs(base_path, &defaults) {
                                        Ok(report) => {
                                            println!(
                                                "Outgoing -> JSON: processed {}, wrote {}, skipped {}, failed {}",
                                                report.processed,
                                                report.written,
                                                report.skipped,
                                                report.failed
                                            );
                                            if let Some(first) = report.errors.first() {
                                                println!("First error: {}", first);
                                            }
                                            if let Ok(Some((_path, pack))) =
                                                find_latest_pack(base_path)
                                            {
                                                apply_pack_policy(settings, &pack);
                                                let _ = save_settings(settings, base_path);
                                            }
                                        }
                                        Err(e) => {
                                            println!("Transcribe failed: {}", e);
                                        }
                                    }
                                }
                                Err(e) => println!("Copy failed: {}", e),
                            }
                        } else {
                            let dest_dir = base_path.join("homework").join("assigned");
                            if let Err(e) = std::fs::create_dir_all(&dest_dir) {
                                println!("Failed to create assigned dir: {}", e);
                                continue;
                            }
                            let dest = dest_dir.join(src.file_name().unwrap_or_else(|| {
                                std::ffi::OsStr::new("homework_pack_import.json")
                            }));
                            match std::fs::copy(&src, &dest) {
                                Ok(_) => match load_pack_from_file(&dest) {
                                    Ok(pack) => {
                                        apply_pack_policy(settings, &pack);
                                        if let Err(e) = save_settings(settings, base_path) {
                                            println!(
                                                "Imported pack but failed to save settings: {}",
                                                e
                                            );
                                        } else {
                                            println!(
                                                "Imported pack to {} and applied policy.",
                                                dest.display()
                                            );
                                        }
                                    }
                                    Err(e) => println!("Copied but failed to parse pack: {}", e),
                                },
                                Err(e) => println!("Copy failed: {}", e),
                            }
                        }
                    }
                }
            }
            "back" => {
                if let Err(e) = save_settings(settings, base_path) {
                    println!("Failed to save settings: {}", e);
                }
                println!("Exiting teacher console.\n");
                break;
            }
            "set_pin" => {
                let new_pin = match prompt("New PIN", "") {
                    Ok(v) => v,
                    Err(e) => {
                        println!("Failed to read PIN: {}", e);
                        continue;
                    }
                };
                let confirm_pin = match prompt("Confirm PIN", "") {
                    Ok(v) => v,
                    Err(e) => {
                        println!("Failed to read PIN confirmation: {}", e);
                        continue;
                    }
                };
                if new_pin.trim().is_empty() {
                    println!("PIN cannot be empty.");
                    continue;
                }
                if new_pin != confirm_pin {
                    println!("PINs did not match. PIN unchanged.");
                    continue;
                }
                settings.teacher_pin = new_pin;
                if let Err(e) = save_settings(settings, base_path) {
                    println!("PIN updated but failed to save settings: {}", e);
                } else {
                    println!("Teacher PIN updated.");
                }
            }
            "set_secret" => {
                let question = match prompt("New secret question", "") {
                    Ok(v) => v,
                    Err(e) => {
                        println!("Failed to read question: {}", e);
                        continue;
                    }
                };
                let answer = match prompt("New secret answer", "") {
                    Ok(v) => v,
                    Err(e) => {
                        println!("Failed to read answer: {}", e);
                        continue;
                    }
                };
                if question.trim().is_empty() || answer.trim().is_empty() {
                    println!("Question and answer cannot be empty.");
                    continue;
                }
                settings.teacher_secret_question = question.trim().to_string();
                settings.teacher_secret_answer = answer.trim().to_string();
                if let Err(e) = save_settings(settings, base_path) {
                    println!("Secret updated but failed to save settings: {}", e);
                } else {
                    println!("Secret question/answer updated.");
                }
            }
            _ => println!("Unknown command."),
        }
    }
}

fn create_pack_interactive(base_path: &Path) -> io::Result<PathBuf> {
    println!("Creating homework pack (single assignment). Leave blank for defaults.");
    let school_id = prompt("School ID", "school")?;
    let class_id = prompt("Class ID", "class")?;
    let assignment_id = prompt("Assignment ID", "hw-001")?;
    let title = prompt("Title", "Homework")?;
    let subject = prompt("Subject", "General")?;
    let year_level = prompt("Year level / grade", "7")?;
    let due_at = prompt("Due at (ISO8601, optional)", "")?;
    let allow_games = prompt("Allow games? (y/n)", "n")?
        .to_lowercase()
        .starts_with('y');
    let allow_ai_premark = prompt("Allow AI premark? (y/n)", "y")?
        .to_lowercase()
        .starts_with('y');
    let max_score = prompt("Max score (int, optional)", "")?;
    let instructions = prompt("Instructions (one line)", "Add details here.")?;

    let assignment = HomeworkAssignment {
        id: assignment_id,
        title,
        subject,
        year_level,
        due_at: if due_at.is_empty() {
            None
        } else {
            Some(due_at)
        },
        instructions_md: instructions,
        student_printable_md: None,
        teacher_rubric_md: None,
        attachments: vec![],
        allow_games,
        allow_ai_premark,
        max_score: if max_score.is_empty() {
            None
        } else {
            max_score.parse().ok()
        },
    };

    create_pack(base_path, &school_id, &class_id, assignment)
}

fn prompt(field: &str, default_val: &str) -> io::Result<String> {
    print!("{} [{}]: ", field, default_val);
    io::stdout().flush().unwrap();
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        Ok(default_val.to_string())
    } else {
        Ok(trimmed.to_string())
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

fn create_pack_multi_interactive(base_path: &Path) -> io::Result<PathBuf> {
    println!("Creating homework pack (multiple assignments). Leave blank for defaults. Enter 'done' for Assignment ID to finish.");
    let school_id = prompt("School ID", "school")?;
    let class_id = prompt("Class ID", "class")?;

    let mut assignments: Vec<HomeworkAssignment> = Vec::new();
    loop {
        let assignment_id = prompt("Assignment ID", "")?;
        if assignment_id.trim().is_empty() || assignment_id.trim().eq_ignore_ascii_case("done") {
            break;
        }
        let title = prompt("Title", "Homework")?;
        let subject = prompt("Subject", "General")?;
        let year_level = prompt("Year level / grade", "7")?;
        let due_at = prompt("Due at (ISO8601, optional)", "")?;
        let allow_games = prompt("Allow games? (y/n)", "n")?
            .to_lowercase()
            .starts_with('y');
        let allow_ai_premark = prompt("Allow AI premark? (y/n)", "y")?
            .to_lowercase()
            .starts_with('y');
        let max_score = prompt("Max score (int, optional)", "")?;
        let instructions = prompt("Instructions (one line)", "Add details here.")?;

        let assignment = HomeworkAssignment {
            id: assignment_id,
            title,
            subject,
            year_level,
            due_at: if due_at.is_empty() {
                None
            } else {
                Some(due_at)
            },
            instructions_md: instructions,
            student_printable_md: None,
            teacher_rubric_md: None,
            attachments: vec![],
            allow_games,
            allow_ai_premark,
            max_score: if max_score.is_empty() {
                None
            } else {
                max_score.parse().ok()
            },
        };
        assignments.push(assignment);
    }

    if assignments.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "No assignments added.",
        ));
    }

    create_pack_multi(base_path, &school_id, &class_id, assignments)
}
