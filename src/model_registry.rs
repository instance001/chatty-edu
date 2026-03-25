use crate::settings::Settings;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DetectedModelFile {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AutoModelRoleResult {
    pub changed: bool,
}

pub fn discover_gguf_models(base: &Path) -> Vec<DetectedModelFile> {
    let models_dir = base.join("models");
    let _ = fs::create_dir_all(&models_dir);

    let mut models = Vec::new();
    let Ok(entries) = fs::read_dir(&models_dir) else {
        return models;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if file_name.starts_with('.') || file_name.eq_ignore_ascii_case("gitkeep") {
            continue;
        }

        let Ok(true) = gguf_magic_ok(&path) else {
            continue;
        };

        let size_bytes = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(file_name)
            .to_string();
        models.push(DetectedModelFile {
            name,
            path,
            size_bytes,
        });
    }

    models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    models
}

pub fn auto_assign_model_roles(settings: &mut Settings, base: &Path) -> AutoModelRoleResult {
    let models = discover_gguf_models(base);
    let mut result = AutoModelRoleResult { changed: false };

    if models.is_empty() {
        result.changed |= settings.model.name != "No model selected";
        result.changed |= !settings.model.path.trim().is_empty();
        result.changed |= settings.bookkeeper_model_name != "Keyword-only background summary mode";
        result.changed |= !settings.bookkeeper_model_path.trim().is_empty();

        settings.model.name = "No model selected".to_string();
        settings.model.path.clear();
        settings.bookkeeper_model_name = "Keyword-only background summary mode".to_string();
        settings.bookkeeper_model_path.clear();
        return result;
    }

    let mut by_size = models.clone();
    by_size.sort_by(|a, b| {
        a.size_bytes
            .cmp(&b.size_bytes)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let smallest = by_size
        .first()
        .cloned()
        .unwrap_or_else(|| models[0].clone());
    let largest = by_size.last().cloned().unwrap_or_else(|| models[0].clone());

    result.changed |= settings.model.name != largest.name;
    result.changed |= settings.model.path != largest.path.to_string_lossy();
    settings.model.name = largest.name.clone();
    settings.model.path = largest.path.to_string_lossy().to_string();

    if by_size.len() >= 2 {
        result.changed |= settings.bookkeeper_model_name != smallest.name;
        result.changed |= settings.bookkeeper_model_path != smallest.path.to_string_lossy();
        settings.bookkeeper_model_name = smallest.name;
        settings.bookkeeper_model_path = smallest.path.to_string_lossy().to_string();
    } else {
        result.changed |= settings.bookkeeper_model_name != "Keyword-only background summary mode";
        result.changed |= !settings.bookkeeper_model_path.trim().is_empty();
        settings.bookkeeper_model_name = "Keyword-only background summary mode".to_string();
        settings.bookkeeper_model_path.clear();
    }

    result
}

pub fn gguf_magic_ok(path: &Path) -> Result<bool, String> {
    let mut file = File::open(path)
        .map_err(|err| format!("Could not open GGUF file {}: {err}", path.display()))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|err| format!("Could not read GGUF header {}: {err}", path.display()))?;
    Ok(&magic == b"GGUF")
}
