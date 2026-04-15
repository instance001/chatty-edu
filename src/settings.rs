use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const APP_FOLDER_NAME: &str = "Chatty-EDU";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JanetConfig {
    pub enabled: bool,
    pub block_swears: bool,
    pub block_mature_topics: bool,
    pub fallback_message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelConfig {
    pub name: String,
    pub path: String,
    pub max_tokens: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub engine: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameConfig {
    pub enabled: bool,
    pub games_in_class_allowed: bool,
    pub available_games: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UiSettings {
    #[serde(default)]
    pub last_theme: Option<String>,
    #[serde(default)]
    pub window_size: Option<(f32, f32)>,
    #[serde(default)]
    pub restore_tabs: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Settings {
    pub version: String,
    pub base_path: String,
    pub mode: String,
    #[serde(default)]
    pub network_device_id: String,
    #[serde(default)]
    pub network_recoverable_shared_chat_policy_json: Option<String>,
    #[serde(default)]
    pub network_device_name: String,
    #[serde(default = "default_true")]
    pub network_allow_unknown_devices: bool,
    #[serde(default = "default_true")]
    pub network_allow_shared_lukewarm_context: bool,
    #[serde(default = "default_true")]
    pub allow_sandbox_tool_requests: bool,
    #[serde(default)]
    pub network_trusted_devices: Vec<StoredNetworkPeer>,
    #[serde(default)]
    pub network_blocked_devices: Vec<StoredNetworkPeer>,
    #[serde(default)]
    pub network_device_aliases: HashMap<String, String>,
    #[serde(default)]
    pub network_device_groups: HashMap<String, String>,
    pub default_year_level: String,
    pub teacher_mode: String,
    #[serde(default = "default_homework_hints_only")]
    pub homework_hints_only: bool,
    #[serde(default = "default_teacher_pin")]
    pub teacher_pin: String,
    #[serde(default = "default_secret_question")]
    pub teacher_secret_question: String,
    #[serde(default = "default_secret_answer")]
    pub teacher_secret_answer: String,
    #[serde(default)]
    pub student: StudentProfile,
    pub janet: JanetConfig,
    pub model: ModelConfig,
    #[serde(default)]
    pub bookkeeper_model_name: String,
    #[serde(default)]
    pub bookkeeper_model_path: String,
    pub voice: VoiceConfig,
    pub game: GameConfig,
    #[serde(default)]
    pub ui: UiSettings,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct StoredNetworkPeer {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub device_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct StudentProfile {
    #[serde(default)]
    pub student_id: String,
    #[serde(default)]
    pub student_name: String,
    #[serde(default)]
    pub class_id: String,
}

pub fn default_teacher_pin() -> String {
    "0000".to_string()
}

pub fn default_secret_question() -> String {
    "What is your favourite school subject?".to_string()
}

pub fn default_secret_answer() -> String {
    "math".to_string()
}

pub fn default_homework_hints_only() -> bool {
    true
}

pub fn default_true() -> bool {
    true
}

pub fn default_base_path() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(project_root) = exe_path
            .ancestors()
            .filter(|path| path.is_dir())
            .find(|path| path.join("Cargo.toml").is_file() && path.join("src").is_dir())
        {
            return project_root.to_path_buf();
        }

        if let Some(dir) = exe_path.parent() {
            return dir.join("data");
        }
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_FOLDER_NAME)
}

pub fn ensure_base_folders(base: &Path) -> io::Result<()> {
    let dirs = [
        base.to_path_buf(),
        base.join("homework"),
        base.join("homework").join("assigned"),
        base.join("homework").join("completed"),
        base.join("homework").join("outgoing"),
        base.join("homework").join("marking"),
        base.join("homework").join("printables"),
        base.join("homework").join("rubrics"),
        base.join("revision"),
        base.join("revision").join("notes"),
        base.join("revision").join("past_papers"),
        base.join("modules"),
        base.join("logs"),
        base.join("config"),
        base.join("config").join("bookkeeper"),
        base.join("runtime"),
        base.join("themes"),
        base.join("models"),
        base.join("Chatty_Sandbox"),
    ];

    for d in dirs {
        if !d.exists() {
            fs::create_dir_all(&d)?;
        }
    }

    Ok(())
}

pub fn settings_path(base: &Path) -> PathBuf {
    base.join("config").join("settings.json")
}

pub fn load_or_init_settings(base: &Path) -> io::Result<Settings> {
    let config_path = settings_path(base);

    if config_path.exists() {
        let contents = fs::read_to_string(&config_path)?;
        let mut settings: Settings = serde_json::from_str(&contents)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("JSON parse error: {e}")))?;

        // Ensure base_path stays in sync with the current base
        if settings.base_path != base.to_string_lossy() {
            settings.base_path = base.to_string_lossy().to_string();
        }
        return Ok(settings);
    }

    let settings = Settings {
        version: "0.5.0".to_string(),
        base_path: base.to_string_lossy().to_string(),
        mode: "gui".to_string(),
        network_device_id: String::new(),
        network_recoverable_shared_chat_policy_json: None,
        network_device_name: String::new(),
        network_allow_unknown_devices: true,
        network_allow_shared_lukewarm_context: true,
        allow_sandbox_tool_requests: true,
        network_trusted_devices: Vec::new(),
        network_blocked_devices: Vec::new(),
        network_device_aliases: HashMap::new(),
        network_device_groups: HashMap::new(),
        default_year_level: "year_3".to_string(),
        teacher_mode: "class".to_string(),
        homework_hints_only: default_homework_hints_only(),
        teacher_pin: default_teacher_pin(),
        teacher_secret_question: default_secret_question(),
        teacher_secret_answer: default_secret_answer(),
        student: StudentProfile {
            student_id: "student-id-placeholder".to_string(),
            student_name: "Student Name".to_string(),
            class_id: "class-placeholder".to_string(),
        },
        janet: JanetConfig {
            enabled: true,
            block_swears: true,
            block_mature_topics: true,
            fallback_message: "Let's switch topics. I'm here for school-safe chat and study tips."
                .to_string(),
        },
        model: ModelConfig {
            name: "phi-mini-placeholder".to_string(),
            path: base
                .join("models")
                .join("model.gguf")
                .to_string_lossy()
                .to_string(),
            max_tokens: 512,
        },
        bookkeeper_model_name: String::new(),
        bookkeeper_model_path: String::new(),
        voice: VoiceConfig {
            enabled: false,
            engine: "os_tts".to_string(),
        },
        game: GameConfig {
            enabled: true,
            games_in_class_allowed: false,
            available_games: vec!["chattybox".to_string(), "chattyclysm".to_string()],
        },
        ui: UiSettings::default(),
    };

    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("JSON encode error: {e}")))?;
    fs::write(&config_path, json)?;

    Ok(settings)
}

pub fn save_settings(settings: &Settings, base: &Path) -> io::Result<()> {
    let config_path = settings_path(base);
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("JSON encode error: {e}")))?;
    fs::write(&config_path, json)?;
    Ok(())
}
