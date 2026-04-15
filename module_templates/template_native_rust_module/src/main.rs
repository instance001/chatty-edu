use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use serde::{Deserialize, Serialize};

const MODULE_ID: &str = "template_native_rust_module";

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Template Native Rust Module")
            .with_inner_size([1024.0, 720.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Template Native Rust Module",
        native_options,
        Box::new(|_cc| Ok(Box::new(TemplateNativeApp::new()))),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ModuleState {
    module_title: String,
    current_focus: String,
    recent_changes: String,
    next_action: String,
    artifacts: String,
}

struct TemplateNativeApp {
    state_path: PathBuf,
    bridge_status_path: Option<PathBuf>,
    last_bridge_fingerprint: String,
    status: String,
    state: ModuleState,
}

impl TemplateNativeApp {
    fn new() -> Self {
        let state_path = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("state.json");
        let mut app = Self {
            state_path,
            bridge_status_path: std::env::var_os("CHATTYEDU_BRIDGE_STATUS").map(PathBuf::from),
            last_bridge_fingerprint: String::new(),
            status: "Ready.".to_string(),
            state: ModuleState::default(),
        };
        app.load_state();
        app
    }

    fn load_state(&mut self) {
        let Ok(bytes) = std::fs::read(&self.state_path) else {
            return;
        };
        if let Ok(state) = serde_json::from_slice::<ModuleState>(&bytes) {
            self.state = state;
        }
    }

    fn save_state(&mut self) {
        match serde_json::to_vec_pretty(&self.state) {
            Ok(bytes) => {
                if std::fs::write(&self.state_path, bytes).is_ok() {
                    self.status = format!("Saved state to {}", self.state_path.display());
                } else {
                    self.status = "Could not save state.".to_string();
                }
            }
            Err(_) => {
                self.status = "Could not serialize state.".to_string();
            }
        }
    }

    fn reset_state(&mut self) {
        self.state = ModuleState::default();
        self.status = "State reset.".to_string();
        self.save_state();
    }

    fn field_completion(&self) -> usize {
        [
            self.state.module_title.trim(),
            self.state.current_focus.trim(),
            self.state.recent_changes.trim(),
            self.state.next_action.trim(),
            self.state.artifacts.trim(),
        ]
        .into_iter()
        .filter(|value| !value.is_empty())
        .count()
    }

    fn artifact_count(&self) -> usize {
        self.state
            .artifacts
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .count()
    }

    fn bridge_summary(&self) -> String {
        let title = if self.state.module_title.trim().is_empty() {
            "Template Native Rust Module"
        } else {
            self.state.module_title.trim()
        };
        let focus = if self.state.current_focus.trim().is_empty() {
            "no current focus set"
        } else {
            self.state.current_focus.trim()
        };
        let next = if self.state.next_action.trim().is_empty() {
            "define the next action"
        } else {
            self.state.next_action.trim()
        };
        let completion = ((self.field_completion() as f32 / 5.0) * 100.0).round() as i32;
        format!(
            "{title} is {completion}% filled out. Current focus: {focus}. Artifacts listed: {}. Next handoff focus: {next}.",
            self.artifact_count()
        )
    }

    fn bridge_snapshot(&self) -> String {
        [
            "# Template Native Rust Module Snapshot".to_string(),
            "".to_string(),
            format!(
                "- Module title: {}",
                if self.state.module_title.trim().is_empty() {
                    "not set"
                } else {
                    self.state.module_title.trim()
                }
            ),
            format!(
                "- Current focus: {}",
                if self.state.current_focus.trim().is_empty() {
                    "not set"
                } else {
                    self.state.current_focus.trim()
                }
            ),
            "".to_string(),
            "## Recent changes".to_string(),
            if self.state.recent_changes.trim().is_empty() {
                "(empty)".to_string()
            } else {
                self.state.recent_changes.trim().to_string()
            },
            "".to_string(),
            "## Next action".to_string(),
            if self.state.next_action.trim().is_empty() {
                "(empty)".to_string()
            } else {
                self.state.next_action.trim().to_string()
            },
            "".to_string(),
            "## Artifacts".to_string(),
            if self.state.artifacts.trim().is_empty() {
                "(none)".to_string()
            } else {
                self.state.artifacts.trim().to_string()
            },
        ]
        .join("\n")
    }

    fn sync_bridge(&mut self) {
        let Some(path) = self.bridge_status_path.clone() else {
            return;
        };

        let body = serde_json::json!({
            "module_id": MODULE_ID,
            "event_type": "suspend_rundown",
            "summary": self.bridge_summary(),
            "snapshot": self.bridge_snapshot(),
            "tags": ["template", "native_window", "starter"],
            "payload": {
                "completion": self.field_completion(),
                "artifact_count": self.artifact_count(),
                "status": self.status,
            },
            "updated_at_unix_ms": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64
        });

        let fingerprint = body.to_string();
        if fingerprint == self.last_bridge_fingerprint {
            return;
        }

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&body) {
            if std::fs::write(path, bytes).is_ok() {
                self.last_bridge_fingerprint = fingerprint;
            }
        }
    }
}

impl eframe::App for TemplateNativeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Save").clicked() {
                    self.save_state();
                }
                if ui.button("Reset").clicked() {
                    self.reset_state();
                }
                ui.separator();
                ui.small(&self.status);
            });
        });

        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.heading("Template Native Module");
                ui.separator();
                ui.label(format!(
                    "Fields filled: {}%",
                    ((self.field_completion() as f32 / 5.0) * 100.0).round() as i32
                ));
                ui.label(format!("Artifacts listed: {}", self.artifact_count()));
                ui.label(if self.bridge_status_path.is_some() {
                    "Bridge mode: hosted"
                } else {
                    "Bridge mode: standalone"
                });
                ui.add_space(8.0);
                ui.label("This starter keeps app state in `state.json` and only reports through the optional Chatty-EDU bridge when hosted.");
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                columns[0].heading("Module desk");
                columns[0].label("Customize these fields, then copy the module and rename it.");
                columns[0].add_space(8.0);
                columns[0].label("Module title");
                columns[0].text_edit_singleline(&mut self.state.module_title);
                columns[0].label("Current focus");
                columns[0].text_edit_singleline(&mut self.state.current_focus);
                columns[0].label("Recent changes");
                columns[0].add(
                    egui::TextEdit::multiline(&mut self.state.recent_changes)
                        .desired_rows(8),
                );
                columns[0].label("Next action");
                columns[0].add(
                    egui::TextEdit::multiline(&mut self.state.next_action)
                        .desired_rows(5),
                );
                columns[0].label("Artifacts");
                columns[0].add(
                    egui::TextEdit::multiline(&mut self.state.artifacts)
                        .desired_rows(6),
                );

                columns[1].heading("Handoff preview");
                columns[1].label("This is the kind of summary/snapshot Chatty-EDU can read when the module is hosted.");
                columns[1].add_space(8.0);
                let mut summary = self.bridge_summary();
                columns[1].label("Summary");
                columns[1].add(
                    egui::TextEdit::multiline(&mut summary)
                        .desired_rows(6)
                        .interactive(false),
                );
                let mut snapshot = self.bridge_snapshot();
                columns[1].label("Snapshot");
                columns[1].add(
                    egui::TextEdit::multiline(&mut snapshot)
                        .desired_rows(18)
                        .interactive(false),
                );
            });
        });

        self.sync_bridge();
    }
}
