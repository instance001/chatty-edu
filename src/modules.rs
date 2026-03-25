use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn default_roles() -> Vec<String> {
    vec!["teacher".to_string(), "student".to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ModuleEntry {
    BuiltinPanel {
        target: String,
    },
    Markdown {
        path: String,
    },
    StaticHtml {
        path: String,
    },
    ExternalProcess {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    #[serde(default = "default_roles")]
    pub roles: Vec<String>,
    pub entry: ModuleEntry,
    pub icon: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub manifest: ModuleManifest,
    pub folder: PathBuf,
}

pub fn load_modules(base: &Path) -> io::Result<Vec<LoadedModule>> {
    let modules_root = base.join("modules");
    ensure_builtin_modules(&modules_root)?;
    let mut results = Vec::new();

    if !modules_root.exists() {
        return Ok(results);
    }

    for entry in fs::read_dir(&modules_root)? {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                eprintln!("[modules] Failed to read entry: {err}");
                continue;
            }
        };

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("module.json");
        if !manifest_path.exists() {
            eprintln!(
                "[modules] Skipping {:?} (no module.json found)",
                path.file_name().unwrap_or_default()
            );
            continue;
        }

        let manifest_str = match fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(err) => {
                eprintln!(
                    "[modules] Could not read {:?}: {}",
                    manifest_path.file_name().unwrap_or_default(),
                    err
                );
                continue;
            }
        };

        match serde_json::from_str::<ModuleManifest>(&manifest_str) {
            Ok(manifest) => {
                results.push(LoadedModule {
                    manifest,
                    folder: path,
                });
            }
            Err(err) => {
                eprintln!(
                    "[modules] Invalid manifest {:?}: {}",
                    manifest_path.file_name().unwrap_or_default(),
                    err
                );
            }
        }
    }

    Ok(results)
}

fn ensure_builtin_modules(modules_root: &Path) -> io::Result<()> {
    fs::create_dir_all(modules_root)?;

    ensure_module(
        modules_root,
        "homework_dashboard",
        ModuleManifest {
            id: "homework_dashboard".to_string(),
            title: "Homework Dashboard".to_string(),
            description: Some("Built-in view for packs and submissions".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Chatty-EDU".to_string()),
            roles: vec!["teacher".to_string()],
            entry: ModuleEntry::BuiltinPanel {
                target: "homework_dashboard".to_string(),
            },
            icon: None,
            permissions: vec![],
        },
    )?;

    ensure_module(
        modules_root,
        "homework_assignments",
        ModuleManifest {
            id: "homework_assignments".to_string(),
            title: "Revision".to_string(),
            description: Some("Practice from completed work and past papers".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Chatty-EDU".to_string()),
            roles: vec!["teacher".to_string(), "student".to_string()],
            entry: ModuleEntry::BuiltinPanel {
                target: "revision_workspace".to_string(),
            },
            icon: None,
            permissions: vec![],
        },
    )?;

    Ok(())
}

fn ensure_module(
    modules_root: &Path,
    folder_name: &str,
    manifest: ModuleManifest,
) -> io::Result<()> {
    let folder = modules_root.join(folder_name);
    let manifest_path = folder.join("module.json");
    fs::create_dir_all(&folder)?;
    let json = serde_json::to_string_pretty(&manifest)?;
    let should_write = match fs::read_to_string(&manifest_path) {
        Ok(existing) => existing != json,
        Err(_) => true,
    };
    if should_write {
        fs::write(&manifest_path, json)?;
    }
    Ok(())
}

pub fn role_allowed(manifest: &ModuleManifest, role: &str) -> bool {
    manifest.roles.iter().any(|r| r.eq_ignore_ascii_case(role))
}
