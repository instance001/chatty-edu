use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModuleCommandSpec {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub is_path_like: bool,
}

impl ResolvedCommand {
    pub fn from_spec(module_dir: &Path, spec: &ModuleCommandSpec) -> Result<Self, String> {
        let program = spec.program.trim();
        if program.is_empty() {
            return Err("Command is missing `program`.".to_string());
        }

        let is_path_like = looks_like_path(program);
        let resolved_program = if is_path_like {
            resolve_relative_path(module_dir, program)
        } else {
            PathBuf::from(program)
        };
        let cwd = Some(
            spec.cwd
                .as_deref()
                .map(|value| resolve_relative_path(module_dir, value))
                .unwrap_or_else(|| module_dir.to_path_buf()),
        );

        Ok(Self {
            program: resolved_program,
            args: spec.args.clone(),
            cwd,
            env: spec.env.clone(),
            is_path_like,
        })
    }

    pub fn display_string(&self) -> String {
        let mut parts = vec![self.program.display().to_string()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }
}

pub fn run_command_to_completion(command: &ResolvedCommand) -> Result<String, String> {
    let mut process = Command::new(&command.program);
    process.args(&command.args);
    if let Some(cwd) = &command.cwd {
        process.current_dir(cwd);
    }
    for (key, value) in &command.env {
        process.env(key, value);
    }
    let output = process
        .output()
        .map_err(|err| format!("Failed to start {}: {err}", command.display_string()))?;

    if output.status.success() {
        Ok(format!(
            "{} completed successfully.",
            command.display_string()
        ))
    } else {
        let mut detail = String::new();
        let stderr = String::from_utf8_lossy(&output.stderr);
        if let Some(line) = stderr.lines().rev().find(|line| !line.trim().is_empty()) {
            detail = line.trim().to_string();
        } else {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = stdout.lines().rev().find(|line| !line.trim().is_empty()) {
                detail = line.trim().to_string();
            }
        }

        if detail.is_empty() {
            Err(format!(
                "{} failed with status {}.",
                command.display_string(),
                output.status
            ))
        } else {
            Err(format!(
                "{} failed with status {}: {}",
                command.display_string(),
                output.status,
                detail
            ))
        }
    }
}

pub fn resolve_webview_target(
    module_dir: &Path,
    url: Option<&str>,
    file: Option<&str>,
) -> Result<String, String> {
    if let Some(url) = url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Ok(url);
    }

    let Some(file) = file
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Err("Webview visual_load requires either `url` or `file`.".to_string());
    };

    let path = resolve_relative_path(module_dir, &file);
    if !path.is_file() {
        return Err(format!("Webview file not found: {}", path.display()));
    }

    url::Url::from_file_path(&path)
        .map(|url| url.to_string())
        .map_err(|_| format!("Failed to convert {} into a file:// URL.", path.display()))
}

pub struct WebviewHostConfig<'a> {
    pub windows_binary_name: &'a str,
    pub other_binary_name: &'a str,
    pub cargo_bin_name: &'a str,
}

pub fn resolve_webview_host_binary(config: &WebviewHostConfig<'_>) -> Result<PathBuf, String> {
    let helper_name = if cfg!(target_os = "windows") {
        config.windows_binary_name
    } else {
        config.other_binary_name
    };

    let candidates = webview_host_candidates(helper_name);
    if let Some(found) = candidates.iter().find(|path| path.is_file()) {
        return Ok(found.clone());
    }

    if let Some(project_root) = find_cargo_project_root() {
        if try_build_webview_host(&project_root, config.cargo_bin_name).is_ok() {
            let candidates = webview_host_candidates(helper_name);
            if let Some(found) = candidates.iter().find(|path| path.is_file()) {
                return Ok(found.clone());
            }
        }
    }

    let preferred = candidates
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from(helper_name));
    Err(format!(
        "Webview host binary not found: {}",
        preferred.display()
    ))
}

fn looks_like_path(value: &str) -> bool {
    value.contains('\\')
        || value.contains('/')
        || value.starts_with('.')
        || value.contains(':')
        || value.to_ascii_lowercase().ends_with(".exe")
        || value.to_ascii_lowercase().ends_with(".bat")
        || value.to_ascii_lowercase().ends_with(".cmd")
        || value.to_ascii_lowercase().ends_with(".py")
}

fn resolve_relative_path(module_dir: &Path, value: &str) -> PathBuf {
    let path = Path::new(value.trim());
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        module_dir.join(path)
    }
}

fn webview_host_candidates(helper_name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let profile_hint = current_profile_hint();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(helper_name));
        }
    }

    if let Some(project_root) = find_cargo_project_root() {
        if let Some(profile) = profile_hint.as_deref() {
            candidates.push(project_root.join("target").join(profile).join(helper_name));
        }
        candidates.push(project_root.join("target").join("debug").join(helper_name));
        candidates.push(project_root.join("target").join("release").join(helper_name));
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Some(profile) = profile_hint.as_deref() {
            candidates.push(cwd.join("target").join(profile).join(helper_name));
        }
        candidates.push(cwd.join("target").join("debug").join(helper_name));
        candidates.push(cwd.join("target").join("release").join(helper_name));
    }

    let mut deduped = Vec::new();
    for candidate in candidates {
        if !deduped.iter().any(|existing: &PathBuf| existing == &candidate) {
            deduped.push(candidate);
        }
    }
    deduped
}

fn current_profile_hint() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = dir.file_name()?.to_string_lossy().to_ascii_lowercase();
    match name.as_str() {
        "debug" => Some("debug".to_string()),
        "release" => Some("release".to_string()),
        _ => None,
    }
}

fn find_cargo_project_root() -> Option<PathBuf> {
    let mut starts = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            starts.push(dir.to_path_buf());
            if let Some(parent) = dir.parent() {
                starts.push(parent.to_path_buf());
            }
        }
    }

    for start in starts {
        let mut current = Some(start.as_path());
        while let Some(dir) = current {
            let cargo_toml = dir.join("Cargo.toml");
            if cargo_toml.is_file() {
                return Some(dir.to_path_buf());
            }
            current = dir.parent();
        }
    }

    None
}

fn try_build_webview_host(project_root: &Path, cargo_bin_name: &str) -> Result<(), String> {
    let mut command = Command::new("cargo");
    command.current_dir(project_root);
    command.arg("build");
    if matches!(current_profile_hint().as_deref(), Some("release")) {
        command.arg("--release");
    }
    command.arg("--bin").arg(cargo_bin_name);

    let output = command.output().map_err(|err| {
        format!(
            "Could not build missing webview helper via cargo in {}: {err}",
            project_root.display()
        )
    })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .or_else(|| stdout.lines().rev().find(|line| !line.trim().is_empty()))
            .unwrap_or("cargo build failed");
        Err(format!(
            "Could not build missing webview helper in {}: {}",
            project_root.display(),
            detail.trim()
        ))
    }
}
