use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn write_chattyedu_bridge_status(
    module_id: &str,
    summary: impl Into<String>,
    snapshot: impl Into<String>,
    tags: &[&str],
    payload: serde_json::Value,
) -> Result<(), String> {
    let Some(path) = std::env::var_os("CHATTYEDU_BRIDGE_STATUS").map(PathBuf::from) else {
        return Ok(());
    };

    let body = serde_json::json!({
        "module_id": module_id,
        "event_type": "suspend_rundown",
        "summary": summary.into(),
        "snapshot": snapshot.into(),
        "tags": tags,
        "payload": payload,
        "updated_at_unix_ms": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    });

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let bytes = serde_json::to_vec_pretty(&body).map_err(|err| err.to_string())?;
    std::fs::write(path, bytes).map_err(|err| err.to_string())
}

pub fn clear_chattyedu_bridge_status() -> Result<(), String> {
    let Some(path) = std::env::var_os("CHATTYEDU_BRIDGE_STATUS").map(PathBuf::from) else {
        return Ok(());
    };
    if path.is_file() {
        std::fs::remove_file(path).map_err(|err| err.to_string())?;
    }
    Ok(())
}
