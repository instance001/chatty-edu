use crate::chat::{generate_answer, generate_answer_with_system_prompt};
use crate::ecg_window::EcgWindowState;
use crate::homework_markdown::{self, PackMdDefaults};
use crate::homework_pack::{
    apply_pack_policy, create_pack_multi, export_pack_template, find_latest_pack,
    load_pack_from_file, load_submission_summaries, save_submission_for_assignment,
    HomeworkAssignment, HomeworkPack, SubmissionSummary,
};
use crate::local_model;
use crate::memory::{
    bookkeeper_dir, load_memory_jogger, memory_jogger_path, ColdLogEntry, EduBookkeeperHandle,
};
use crate::model_registry::{discover_gguf_models, gguf_magic_ok};
use crate::module_bridge::{
    bridge_incoming_asset_lane_dir, bridge_incoming_assets_dir, bridge_incoming_shared_state_path,
    bridge_log_sources_path, bridge_outgoing_room_events_path, bridge_shared_room_events_path,
    bridge_shared_room_state_path, bridge_shared_state_path, bridge_status_path,
    clear_bridge_outgoing_room_events, clear_bridge_shared_room_events,
    clear_bridge_shared_room_state, read_bridge_incoming_assets, read_bridge_incoming_shared_state,
    read_bridge_log_excerpts, read_bridge_outgoing_room_events, read_bridge_shared_room_events,
    read_bridge_shared_room_state, read_bridge_shared_state, read_bridge_status,
    write_bridge_incoming_asset, write_bridge_incoming_shared_state,
    write_bridge_shared_room_events, write_bridge_shared_room_state, write_bridge_shared_state,
    ModuleBridgeIncomingAssetRecord, ModuleBridgeIncomingSharedState, ModuleBridgeRoomEvent,
    ModuleBridgeSharedRoomEvents, ModuleBridgeSharedRoomParticipant, ModuleBridgeSharedRoomState,
    ModuleBridgeSharedState,
};
use crate::module_host::{HostRect, ModuleHostState, ModuleVisualLoad};
use crate::modules::{
    load_modules, role_allowed, LoadedModule, ModuleEntry, ModuleNetworkAssetLane,
    ModuleNetworkFeature,
};
use crate::networking::{
    BlockedPeer, LocalPresence, NetworkController, ReceivedArtifact, TrustedPeer,
};
use crate::revision::{
    build_revision_pack_markdown, import_past_paper, load_past_papers, load_revision_progress,
    load_revision_sources, revision_dir, revision_past_papers_dir, revision_priority,
    save_revision_progress, RevisionProgress, RevisionSource,
};
use crate::sandbox::{
    build_recent_chat_prompt_context, build_sandbox_prompt_context, build_task_ledger_prompt_nudge,
    build_task_ledger_user_hint, ensure_default_sandbox_scratchpad_file,
    ensure_default_sandbox_task_ledger_file,
    ensure_path_within_dir as ensure_sandbox_path_within_dir,
    ensure_sandbox_dir as ensure_chatty_edu_sandbox_dir,
    ensure_save_path_within_dir as ensure_sandbox_save_path_within_dir,
    extract_sandbox_actions_from_text, list_sandbox_files, read_task_ledger_summary,
    sandbox_append, sandbox_list, sandbox_preload, sandbox_read, sandbox_write,
    sandbox_write_task_ledger, truncate_for_ui, SandboxAction, TaskLedgerSummary,
    DEFAULT_SANDBOX_SCRATCHPAD_REL_PATH, DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH,
};
use crate::settings::{save_settings, GameConfig, JanetConfig, Settings, VoiceConfig};
use crate::theme::{
    apply_theme, ensure_theme_files, load_presets, load_theme, save_theme, ThemeConfig,
};
use chrono::Utc;
use deunicode::deunicode;
use eframe::{
    egui::{
        self, menu, scroll_area::ScrollBarVisibility, Align, CentralPanel, Context, Layout,
        ProgressBar, RichText, ScrollArea, TextureHandle, TopBottomPanel,
    },
    App, CreationContext,
};
use image::ImageReader;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::panic;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const CHAT_CAPSULE: &str = "Chatty-EDU - Chat Capsule (Chat tab system prompt)\n\
Role: You are Chatty-EDU, an offline learning assistant running entirely on a local computer. You do not have internet access and never browse, search, or fetch links.\n\
Scope: Help with learning questions, explanations, and clarification. Keep responses short, clear, and factual. Default to one concise response unless the user asks for more detail.\n\
Style: Do not invent conversations, roles, or dialogue. Do not hallucinate prior context or role-play multiple speakers. Avoid rambling, repetition, or motivational speeches.\n\
Safety: Use school-appropriate language. If something is outside scope or inappropriate, give one calm sentence that you cannot help and suggest a safe alternative.\n\
Defaults: If you are unsure what the user wants, ask one short clarifying question. If asked what you can do, briefly explain your learning-help role.\n";

const REVISION_CHAT_CAPSULE: &str = "Chatty-EDU - Revision Helper Capsule\n\
Role: You are Chatty-EDU Revision Helper. The student is revising work that has already been submitted.\n\
Scope: Help the student understand past work, revisit mistakes, explain concepts, practice similar questions, and build confidence.\n\
Style: You may explain more openly than the live homework helper because the assignment has already been submitted, but still prefer teaching, reasoning, and mini-examples over blunt answer dumping.\n\
Format: Keep replies short, clear, and useful. A brief explanation, short worked idea, checklist, or quiz question is good.\n\
Social cues: If the student only says a short social cue like \"thanks\", \"thank you\", \"got it\", or \"bye\", reply with one short warm acknowledgement and do not restart the lesson.\n\
Privacy: Never mention internal revision priority, AI scores, saved confidence numbers, or diagnostic feedback labels to the student. Use background signals silently.\n\
Tone: Neutral, supportive, and practical.\n";

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

const FMI_SPLASH_DURATION: Duration = Duration::from_millis(3000);
const FMI_SPLASH_TEXTURE_ID: &str = "chatty_edu_fmi_startup_splash";
const FMI_SPLASH_RELATIVE_PATH: &str = "assets/branding/fmi-splash-wordmark.png";

#[derive(Debug, Clone, Default)]
struct AssignmentDraft {
    id: String,
    title: String,
    subject: String,
    year_level: String,
    due_at: String,
    instructions_md: String,
    allow_games: bool,
    allow_ai_premark: bool,
    max_score: String,
}

#[derive(Debug, Clone)]
struct StudentScore {
    #[allow(dead_code)]
    student_id: String,
    student_name: String,
    subject: String,
    score: f32, // 0-100
}

#[derive(Debug, Clone)]
struct SubmissionRow {
    #[allow(dead_code)]
    assignment_id: String,
    assignment_title: String,
    student_id: String,
    student_name: String,
    subject: String,
    score: String,
    feedback: String,
    #[allow(dead_code)]
    submitted_at: String,
}

#[derive(Debug, Clone)]
enum TabKind {
    Home,
    Chat,
    Sandbox,
    Bookkeeper,
    Networking,
    Settings,
    Diagnostics,
    Module {
        module: LoadedModule,
        cached_text: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct Tab {
    id: usize,
    title: String,
    kind: TabKind,
    closable: bool,
    key: String,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum DiagnosticsAudience {
    StudentSafe,
    Teacher,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum NetworkingQuickHelpMode {
    Everyday,
    TeacherFlow,
    ApprovalFirst,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum NetworkingFocusSection {
    Controls,
    PendingRequests,
    DeviceList,
    SharedRoom,
}

#[derive(Debug, Clone)]
struct LocalModelFile {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct HomeworkQuestionIntercept {
    assignment_id: String,
    question_number: Option<usize>,
    question_text: String,
    normalized_question: String,
    tokens: Vec<String>,
    keyword_tokens: Vec<String>,
    number_tokens: Vec<String>,
    signature_phrases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceivedHomeworkPackRecord {
    artifact_id: String,
    from_device_id: String,
    from_device_name: String,
    label: String,
    summary: String,
    file_name: String,
    received_at_unix_ms: u64,
    pack: HomeworkPack,
}

#[derive(Debug, Clone)]
struct ReceivedHomeworkPackInboxItem {
    path: PathBuf,
    record: ReceivedHomeworkPackRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceivedRevisionPackRecord {
    artifact_id: String,
    from_device_id: String,
    from_device_name: String,
    label: String,
    summary: String,
    file_name: String,
    received_at_unix_ms: u64,
    markdown: String,
}

#[derive(Debug, Clone)]
struct ReceivedRevisionPackInboxItem {
    path: PathBuf,
    record: ReceivedRevisionPackRecord,
}

#[derive(Debug, Clone, Default)]
struct ModuleSessionTracker {
    session_id: String,
    last_revision: u64,
    last_fingerprint: String,
    last_shared_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RecoverableModuleSessionSnapshot {
    session_id: String,
    session_label: String,
    scope_module_id: String,
    scope_module_name: String,
    saved_at_unix_ms: u64,
    latest_shared_state: Option<RecoverableModuleSharedStateSnapshot>,
    recent_assets: Vec<RecoverableModuleSessionAssetSnapshot>,
}

impl RecoverableModuleSessionSnapshot {
    fn normalize(&mut self) {
        self.session_id = self.session_id.trim().to_string();
        self.session_label = self.session_label.trim().to_string();
        self.scope_module_id = self.scope_module_id.trim().to_string();
        self.scope_module_name = self.scope_module_name.trim().to_string();
        if self.saved_at_unix_ms == 0 {
            self.saved_at_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
        }
        if let Some(shared_state) = &mut self.latest_shared_state {
            shared_state.normalize();
        }
        for asset in &mut self.recent_assets {
            asset.normalize();
        }
        self.recent_assets
            .retain(|asset| !asset.cached_payload_name.is_empty());
        self.recent_assets
            .sort_by(|left, right| right.stored_at_unix_ms.cmp(&left.stored_at_unix_ms));
        self.recent_assets.truncate(12);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RecoverableModuleSharedStateSnapshot {
    summary: String,
    session_revision: u64,
    cached_payload_name: String,
    updated_at_unix_ms: u64,
}

impl RecoverableModuleSharedStateSnapshot {
    fn normalize(&mut self) {
        self.summary = self.summary.trim().to_string();
        self.cached_payload_name = self.cached_payload_name.trim().to_string();
        if self.updated_at_unix_ms == 0 {
            self.updated_at_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RecoverableModuleSessionAssetSnapshot {
    artifact_kind: String,
    label: String,
    summary: String,
    file_name: String,
    content_type: String,
    byte_len: u64,
    binary: bool,
    cached_payload_name: String,
    stored_at_unix_ms: u64,
}

impl RecoverableModuleSessionAssetSnapshot {
    fn normalize(&mut self) {
        self.artifact_kind = self.artifact_kind.trim().to_string();
        self.label = self.label.trim().to_string();
        self.summary = self.summary.trim().to_string();
        self.file_name = self.file_name.trim().to_string();
        self.content_type = self.content_type.trim().to_string();
        self.cached_payload_name = self.cached_payload_name.trim().to_string();
        if self.stored_at_unix_ms == 0 {
            self.stored_at_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModuleSessionAckRecord {
    module_id: String,
    session_id: String,
    session_revision: u64,
    from_device_id: String,
    from_device_name: String,
    applied: bool,
    stale: bool,
    message: String,
    acknowledged_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowBundle {
    version: String,
    label: String,
    summary: String,
    created_at_unix_ms: u64,
    teacher_mode: String,
    default_year_level: String,
    homework_hints_only: bool,
    janet: JanetConfig,
    model_hint: Option<String>,
    model_name: String,
    model_max_tokens: u32,
    bookkeeper_model_hint: Option<String>,
    bookkeeper_model_name: String,
    voice: VoiceConfig,
    game: GameConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceivedWorkflowBundleRecord {
    artifact_id: String,
    from_device_id: String,
    from_device_name: String,
    label: String,
    summary: String,
    file_name: String,
    received_at_unix_ms: u64,
    bundle: WorkflowBundle,
}

#[derive(Debug, Clone)]
struct ReceivedWorkflowBundleInboxItem {
    path: PathBuf,
    record: ReceivedWorkflowBundleRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SharedLukewarmContext {
    version: String,
    label: String,
    summary: String,
    created_at_unix_ms: u64,
    source_app: String,
    source_device_id: String,
    source_device_name: String,
    context_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceivedLukewarmContextRecord {
    artifact_id: String,
    from_device_id: String,
    from_device_name: String,
    label: String,
    summary: String,
    file_name: String,
    received_at_unix_ms: u64,
    context: SharedLukewarmContext,
}

#[derive(Debug, Clone)]
struct ReceivedLukewarmContextInboxItem {
    path: PathBuf,
    record: ReceivedLukewarmContextRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceivedGenericTransferLaneDelivery {
    #[serde(default)]
    lane_id: String,
    #[serde(default)]
    lane_label: String,
    #[serde(default)]
    delivered_at_unix_ms: u64,
    #[serde(default)]
    bridge_record_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReceivedGenericTransferRecord {
    artifact_id: String,
    from_device_id: String,
    from_device_name: String,
    label: String,
    summary: String,
    kind: String,
    module_id: String,
    file_name: String,
    content_type: String,
    transfer_encoding: String,
    byte_len: u64,
    chunk_count: u32,
    received_at_unix_ms: u64,
    binary: bool,
    payload_file_name: String,
    preview_text: String,
    #[serde(default)]
    delivered_lanes: Vec<ReceivedGenericTransferLaneDelivery>,
}

#[derive(Debug, Clone)]
struct ReceivedGenericTransferInboxItem {
    path: PathBuf,
    record: ReceivedGenericTransferRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkPeerExchangeRecord {
    device_id: String,
    #[serde(default)]
    device_name: String,
    #[serde(default)]
    alias: String,
    #[serde(default)]
    group: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkPeerExchangeFile {
    version: String,
    source_app: String,
    source_device_id: String,
    source_device_name: String,
    exported_at_unix_ms: u64,
    peers: Vec<NetworkPeerExchangeRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SharedChatTurnMode {
    Open,
    TalkingStick,
}

impl Default for SharedChatTurnMode {
    fn default() -> Self {
        Self::Open
    }
}

impl SharedChatTurnMode {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::TalkingStick => "Talking stick",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SharedChatAiMode {
    Off,
    LocalAllowed,
    HostOnly,
}

impl Default for SharedChatAiMode {
    fn default() -> Self {
        Self::LocalAllowed
    }
}

impl SharedChatAiMode {
    fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::LocalAllowed => "Local allowed",
            Self::HostOnly => "Host only",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum SharedChatScopeKind {
    General,
    Module,
}

impl Default for SharedChatScopeKind {
    fn default() -> Self {
        Self::General
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SharedChatPolicy {
    version: String,
    label: String,
    updated_at_unix_ms: u64,
    source_app: String,
    host_device_id: String,
    host_device_name: String,
    #[serde(default)]
    turn_mode: SharedChatTurnMode,
    #[serde(default)]
    ai_mode: SharedChatAiMode,
    #[serde(default)]
    scope_kind: SharedChatScopeKind,
    #[serde(default)]
    scope_module_id: String,
    #[serde(default)]
    scope_module_name: String,
    #[serde(default)]
    scope_multiplayer: bool,
    #[serde(default)]
    session_active: bool,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    session_revision: u64,
    #[serde(default)]
    session_label: String,
    #[serde(default)]
    host_authoritative: bool,
    #[serde(default)]
    turn_holder_device_id: String,
    #[serde(default)]
    turn_holder_device_name: String,
    #[serde(default)]
    teacher_override: bool,
    #[serde(default)]
    host_activity_state: String,
    #[serde(default)]
    host_activity_label: String,
    #[serde(default)]
    host_activity_updated_at_unix_ms: u64,
}

impl Default for SharedChatPolicy {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            label: "Classroom room".to_string(),
            updated_at_unix_ms: 0,
            source_app: "chatty-edu".to_string(),
            host_device_id: String::new(),
            host_device_name: String::new(),
            turn_mode: SharedChatTurnMode::Open,
            ai_mode: SharedChatAiMode::LocalAllowed,
            scope_kind: SharedChatScopeKind::General,
            scope_module_id: String::new(),
            scope_module_name: String::new(),
            scope_multiplayer: false,
            session_active: false,
            session_id: String::new(),
            session_revision: 0,
            session_label: String::new(),
            host_authoritative: false,
            turn_holder_device_id: String::new(),
            turn_holder_device_name: String::new(),
            teacher_override: false,
            host_activity_state: String::new(),
            host_activity_label: String::new(),
            host_activity_updated_at_unix_ms: 0,
        }
    }
}

impl SharedChatPolicy {
    fn equivalent_except_presence(&self, other: &Self) -> bool {
        self.version == other.version
            && self.label == other.label
            && self.source_app == other.source_app
            && self.host_device_id == other.host_device_id
            && self.host_device_name == other.host_device_name
            && self.turn_mode == other.turn_mode
            && self.ai_mode == other.ai_mode
            && self.scope_kind == other.scope_kind
            && self.scope_module_id == other.scope_module_id
            && self.scope_module_name == other.scope_module_name
            && self.scope_multiplayer == other.scope_multiplayer
            && self.session_active == other.session_active
            && self.session_id == other.session_id
            && self.session_revision == other.session_revision
            && self.session_label == other.session_label
            && self.host_authoritative == other.host_authoritative
            && self.turn_holder_device_id == other.turn_holder_device_id
            && self.turn_holder_device_name == other.turn_holder_device_name
            && self.teacher_override == other.teacher_override
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SharedChatMessage {
    version: String,
    message_id: String,
    sent_at_unix_ms: u64,
    source_app: String,
    from_device_id: String,
    from_device_name: String,
    speaker_kind: String,
    speaker_label: String,
    #[serde(default)]
    scope_kind: SharedChatScopeKind,
    #[serde(default)]
    scope_module_id: String,
    #[serde(default)]
    scope_module_name: String,
    #[serde(default)]
    scope_multiplayer: bool,
    #[serde(default)]
    session_active: bool,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    session_revision: u64,
    body: String,
}

fn discover_local_models(base: &Path) -> Vec<LocalModelFile> {
    discover_gguf_models(base)
        .into_iter()
        .map(|model| LocalModelFile {
            name: model.name,
            path: model.path,
        })
        .collect()
}

pub struct ChattyApp {
    pub settings: Settings,
    base_path: PathBuf,
    modules: Vec<LoadedModule>,
    tabs: Vec<Tab>,
    active_tab: usize,
    next_tab_id: usize,
    chat_input: String,
    chat_log: Vec<(String, String)>,
    memory_jogger: String,
    sandbox_dir: Option<PathBuf>,
    sandbox_selected: Option<PathBuf>,
    sandbox_editor_path: Option<PathBuf>,
    sandbox_last_working_path: Option<PathBuf>,
    sandbox_editor_text: String,
    sandbox_status: String,
    sandbox_action_status: String,
    sandbox_last_tool_result: String,
    sandbox_task_nudge: String,
    pending_sandbox_actions: Vec<SandboxAction>,
    bookkeeper: Option<EduBookkeeperHandle>,
    bookkeeper_query: String,
    bookkeeper_results: Vec<ColdLogEntry>,
    bookkeeper_status: Option<String>,
    theme: ThemeConfig,
    ecg_window: EcgWindowState,
    presets: Vec<ThemeConfig>,
    allow_external_process: bool,
    current_pack: Option<HomeworkPack>,
    received_homework_inbox: Vec<ReceivedHomeworkPackInboxItem>,
    selected_received_homework_pack: Option<PathBuf>,
    received_revision_inbox: Vec<ReceivedRevisionPackInboxItem>,
    selected_received_revision_pack: Option<PathBuf>,
    received_lukewarm_inbox: Vec<ReceivedLukewarmContextInboxItem>,
    selected_received_lukewarm: Option<PathBuf>,
    received_transfer_inbox: Vec<ReceivedGenericTransferInboxItem>,
    selected_received_transfer: Option<PathBuf>,
    received_bundle_inbox: Vec<ReceivedWorkflowBundleInboxItem>,
    selected_received_bundle: Option<PathBuf>,
    submissions: Vec<SubmissionSummary>,
    selected_assignment: Option<String>,
    submission_text: String,
    draft_assignments: Vec<HomeworkAssignment>,
    draft_input: AssignmentDraft,
    selected_students: HashSet<String>,
    submission_attachments: Vec<String>,
    available_models: Vec<LocalModelFile>,
    teacher_unlocked: bool,
    teacher_pin_input: String,
    teacher_pin_new: String,
    teacher_pin_confirm: String,
    teacher_pin_status: Option<String>,
    teacher_secret_answer_input: String,
    teacher_secret_question_input: String,
    revision_sources: Vec<RevisionSource>,
    revision_progress: HashMap<String, RevisionProgress>,
    selected_revision: Option<String>,
    revision_notes: String,
    revision_confidence: i32,
    revision_status: Option<String>,
    revision_chat_input: String,
    revision_chat_log: Vec<(String, String)>,
    past_papers: Vec<PathBuf>,
    teacher_pack_request: String,
    teacher_pack_markdown: String,
    teacher_pack_status: Option<String>,
    teacher_tools_status: Option<String>,
    teacher_revision_status: Option<String>,
    diagnostics_report: String,
    diagnostics_audience: DiagnosticsAudience,
    diagnostics_status: Option<String>,
    homework_question_index: Vec<HomeworkQuestionIntercept>,
    module_hosts: HashMap<String, ModuleHostState>,
    module_host_targets: HashMap<String, HostRect>,
    close_pending_modules: HashSet<String>,
    module_session_trackers: HashMap<String, ModuleSessionTracker>,
    module_session_receipts: Vec<ModuleSessionAckRecord>,
    module_room_bridge_last_fingerprint: HashMap<String, String>,
    module_room_events_bridge_last_fingerprint: HashMap<String, String>,
    networking: NetworkController,
    networking_device_name_input: String,
    networking_status: Option<String>,
    networking_filter: String,
    networking_selected_devices: HashSet<String>,
    networking_turn_holder: Option<String>,
    networking_help_mode: NetworkingQuickHelpMode,
    networking_focus_section: Option<NetworkingFocusSection>,
    networking_focus_pending: Option<NetworkingFocusSection>,
    networking_focus_flash_until: Option<Instant>,
    networking_alias_edit_device: Option<String>,
    networking_alias_input: String,
    networking_group_edit_device: Option<String>,
    networking_group_input: String,
    networking_bundle_label: String,
    networking_bundle_summary: String,
    networking_handoff_target: String,
    networking_handoff_title: String,
    networking_handoff_body: String,
    networking_shared_chat_policy: SharedChatPolicy,
    networking_recoverable_shared_chat_policy: Option<SharedChatPolicy>,
    networking_recoverable_module_session: Option<RecoverableModuleSessionSnapshot>,
    networking_shared_chat_log: Vec<SharedChatMessage>,
    networking_shared_chat_seen_messages: HashSet<String>,
    networking_shared_chat_input: String,
    networking_shared_chat_mirror_main_chat: bool,
    networking_shared_chat_presence_key: String,
    networking_shared_chat_connected_peer_keys: HashSet<String>,
    networking_shared_chat_presence_next_sync_at: Option<Instant>,
    networking_seen_handoffs: HashSet<String>,
    networking_seen_artifacts: HashSet<String>,
    startup_splash_active: bool,
    startup_splash_started_at: Instant,
    startup_splash_texture: Option<TextureHandle>,
}

impl ChattyApp {
    pub fn new(
        cc: &CreationContext<'_>,
        base_path: PathBuf,
        settings: Settings,
    ) -> io::Result<Self> {
        ensure_theme_files(&base_path)?;
        let presets = load_presets(&base_path);
        let theme = load_theme(&base_path, settings.ui.last_theme.as_deref());
        apply_theme(&theme, &cc.egui_ctx);
        let networking = NetworkController::new_with_identity(
            (!settings.network_device_id.trim().is_empty())
                .then(|| settings.network_device_id.clone()),
        );

        let modules = load_modules(&base_path).unwrap_or_default();
        let models = discover_local_models(&base_path);
        let pack = find_latest_pack(&base_path)
            .ok()
            .flatten()
            .map(|(_p, pack)| pack);
        let received_homework_inbox =
            load_received_homework_pack_inbox(&base_path).unwrap_or_default();
        let selected_received_homework_pack = received_homework_inbox
            .first()
            .map(|item| item.path.clone());
        let received_revision_inbox =
            load_received_revision_pack_inbox(&base_path).unwrap_or_default();
        let selected_received_revision_pack = received_revision_inbox
            .first()
            .map(|item| item.path.clone());
        let received_lukewarm_inbox = load_received_lukewarm_inbox(&base_path).unwrap_or_default();
        let selected_received_lukewarm = received_lukewarm_inbox
            .first()
            .map(|item| item.path.clone());
        let received_transfer_inbox =
            load_received_generic_transfer_inbox(&base_path).unwrap_or_default();
        let selected_received_transfer = received_transfer_inbox
            .first()
            .map(|item| item.path.clone());
        let received_bundle_inbox =
            load_received_workflow_bundle_inbox(&base_path).unwrap_or_default();
        let selected_received_bundle = received_bundle_inbox.first().map(|item| item.path.clone());
        let memory_jogger = load_memory_jogger(&base_path);
        let sandbox_dir = ensure_chatty_edu_sandbox_dir(&base_path).ok();
        let bookkeeper = EduBookkeeperHandle::start(&base_path);
        let submissions = load_submission_summaries(&base_path).unwrap_or_default();
        let initial_selected = pack.as_ref().and_then(|p| {
            Self::unique_assignments_by_id(p)
                .first()
                .map(|a| a.id.clone())
        });
        let teacher_secret_question = settings.teacher_secret_question.clone();
        let startup_splash_texture = load_local_png_texture(
            &cc.egui_ctx,
            &base_path.join(FMI_SPLASH_RELATIVE_PATH),
            FMI_SPLASH_TEXTURE_ID,
        )
        .ok();

        let mut app = Self {
            settings,
            base_path,
            modules,
            tabs: vec![
                Tab {
                    id: 0,
                    title: "Home".to_string(),
                    kind: TabKind::Home,
                    closable: false,
                    key: "home".to_string(),
                },
                Tab {
                    id: 1,
                    title: "Chat".to_string(),
                    kind: TabKind::Chat,
                    closable: false,
                    key: "chat".to_string(),
                },
                Tab {
                    id: 2,
                    title: "Sandbox".to_string(),
                    kind: TabKind::Sandbox,
                    closable: false,
                    key: "sandbox".to_string(),
                },
            ],
            active_tab: 0,
            next_tab_id: 3,
            chat_input: String::new(),
            chat_log: Vec::new(),
            memory_jogger,
            sandbox_dir,
            sandbox_selected: None,
            sandbox_editor_path: None,
            sandbox_last_working_path: None,
            sandbox_editor_text: String::new(),
            sandbox_status: String::new(),
            sandbox_action_status: String::new(),
            sandbox_last_tool_result: String::new(),
            sandbox_task_nudge: String::new(),
            pending_sandbox_actions: Vec::new(),
            bookkeeper,
            bookkeeper_query: String::new(),
            bookkeeper_results: Vec::new(),
            bookkeeper_status: None,
            theme,
            ecg_window: EcgWindowState::new("ECG Window - System hardware activity"),
            presets,
            allow_external_process: false,
            current_pack: pack,
            received_homework_inbox,
            selected_received_homework_pack,
            received_revision_inbox,
            selected_received_revision_pack,
            received_lukewarm_inbox,
            selected_received_lukewarm,
            received_transfer_inbox,
            selected_received_transfer,
            received_bundle_inbox,
            selected_received_bundle,
            submissions,
            selected_assignment: initial_selected,
            submission_text: String::new(),
            draft_assignments: Vec::new(),
            draft_input: AssignmentDraft {
                id: "hw-001".to_string(),
                title: "Homework title".to_string(),
                subject: "General".to_string(),
                year_level: "7".to_string(),
                due_at: "".to_string(),
                instructions_md: "Add instructions here.".to_string(),
                allow_games: false,
                allow_ai_premark: true,
                max_score: "100".to_string(),
            },
            selected_students: HashSet::new(),
            submission_attachments: Vec::new(),
            available_models: models,
            teacher_unlocked: false,
            teacher_pin_input: String::new(),
            teacher_pin_new: String::new(),
            teacher_pin_confirm: String::new(),
            teacher_pin_status: None,
            teacher_secret_answer_input: String::new(),
            teacher_secret_question_input: teacher_secret_question,
            revision_sources: Vec::new(),
            revision_progress: HashMap::new(),
            selected_revision: None,
            revision_notes: String::new(),
            revision_confidence: 50,
            revision_status: None,
            revision_chat_input: String::new(),
            revision_chat_log: Vec::new(),
            past_papers: Vec::new(),
            teacher_pack_request: String::new(),
            teacher_pack_markdown: String::new(),
            teacher_pack_status: None,
            teacher_tools_status: None,
            teacher_revision_status: None,
            diagnostics_report: String::new(),
            diagnostics_audience: DiagnosticsAudience::StudentSafe,
            diagnostics_status: None,
            homework_question_index: Vec::new(),
            module_hosts: HashMap::new(),
            module_host_targets: HashMap::new(),
            close_pending_modules: HashSet::new(),
            module_session_trackers: HashMap::new(),
            module_session_receipts: Vec::new(),
            module_room_bridge_last_fingerprint: HashMap::new(),
            module_room_events_bridge_last_fingerprint: HashMap::new(),
            networking,
            networking_device_name_input: String::new(),
            networking_status: None,
            networking_filter: String::new(),
            networking_selected_devices: HashSet::new(),
            networking_turn_holder: None,
            networking_help_mode: NetworkingQuickHelpMode::TeacherFlow,
            networking_focus_section: None,
            networking_focus_pending: None,
            networking_focus_flash_until: None,
            networking_alias_edit_device: None,
            networking_alias_input: String::new(),
            networking_group_edit_device: None,
            networking_group_input: String::new(),
            networking_bundle_label: String::new(),
            networking_bundle_summary: String::new(),
            networking_handoff_target: String::new(),
            networking_handoff_title: String::new(),
            networking_handoff_body: String::new(),
            networking_shared_chat_policy: SharedChatPolicy::default(),
            networking_recoverable_shared_chat_policy: None,
            networking_recoverable_module_session: None,
            networking_shared_chat_log: Vec::new(),
            networking_shared_chat_seen_messages: HashSet::new(),
            networking_shared_chat_input: String::new(),
            networking_shared_chat_mirror_main_chat: false,
            networking_shared_chat_presence_key: String::new(),
            networking_shared_chat_connected_peer_keys: HashSet::new(),
            networking_shared_chat_presence_next_sync_at: Some(Instant::now()),
            networking_seen_handoffs: HashSet::new(),
            networking_seen_artifacts: HashSet::new(),
            startup_splash_active: true,
            startup_splash_started_at: Instant::now(),
            startup_splash_texture,
        };
        if !app.settings.network_device_name.trim().is_empty() {
            let saved_name = app.settings.network_device_name.clone();
            app.networking.set_device_name(&saved_name);
        }
        app.networking
            .set_allow_unknown_devices(app.settings.network_allow_unknown_devices);
        let blocked = app
            .settings
            .network_blocked_devices
            .iter()
            .map(|peer| BlockedPeer {
                device_id: peer.device_id.clone(),
                device_name: peer.device_name.clone(),
                address: String::new(),
                last_seen_secs_ago: None,
            })
            .collect::<Vec<_>>();
        app.networking.replace_blocked_peers(&blocked);
        let trusted = app
            .settings
            .network_trusted_devices
            .iter()
            .map(|peer| TrustedPeer {
                device_id: peer.device_id.clone(),
                device_name: peer.device_name.clone(),
                address: String::new(),
                last_seen_secs_ago: None,
            })
            .collect::<Vec<_>>();
        app.networking.replace_trusted_peers(&trusted);
        app.networking_device_name_input = app.networking.snapshot().device_name.clone();
        if app.settings.network_device_id.trim() != app.networking.snapshot().device_id.trim() {
            app.settings.network_device_id = app.networking.snapshot().device_id.clone();
            let _ = save_settings(&app.settings, &app.base_path);
        }
        app.ensure_shared_chat_policy_defaults();
        app.load_recoverable_shared_chat_policy();
        app.load_recoverable_module_session_snapshot();
        app.ensure_default_sandbox_scratchpad();
        app.ensure_default_sandbox_task_ledger();
        app.refresh_homework_question_index();
        app.resync_revision();
        Ok(app)
    }

    fn reload_modules(&mut self) {
        self.modules = load_modules(&self.base_path).unwrap_or_default();
        self.pulse_ecg(18.0, "Reloaded module manifests.");
    }

    fn reload_models(&mut self) {
        self.available_models = discover_local_models(&self.base_path);
        self.pulse_ecg(18.0, "Rescanned local model files.");
    }

    fn select_model(&mut self, model: &LocalModelFile) {
        self.settings.model.name = model.name.clone();
        self.settings.model.path = model.path.to_string_lossy().to_string();
        local_model::clear_cached_model();
        if let Err(e) = save_settings(&self.settings, &self.base_path) {
            eprintln!("[models] Failed to save selected model: {e}");
        }
        self.pulse_ecg(34.0, "Switched the active local model.");
    }

    fn persist_network_settings(&mut self) {
        if let Err(err) = save_settings(&self.settings, &self.base_path) {
            self.networking_status = Some(format!("Could not save networking settings: {err}"));
        }
    }

    fn normalize_recoverable_shared_chat_policy(mut policy: SharedChatPolicy) -> SharedChatPolicy {
        policy.updated_at_unix_ms = 0;
        policy.host_activity_state.clear();
        policy.host_activity_label.clear();
        policy.host_activity_updated_at_unix_ms = 0;
        policy
    }

    fn load_recoverable_shared_chat_policy(&mut self) {
        self.networking_recoverable_shared_chat_policy = self
            .settings
            .network_recoverable_shared_chat_policy_json
            .as_ref()
            .and_then(|text| serde_json::from_str::<SharedChatPolicy>(text).ok())
            .map(Self::normalize_recoverable_shared_chat_policy)
            .filter(|policy| policy.session_active && !policy.session_id.trim().is_empty());
    }

    fn load_recoverable_module_session_snapshot(&mut self) {
        let path = self.recoverable_module_session_path();
        let had_snapshot = path.is_file();
        let mut snapshot = if had_snapshot {
            fs::read(&path).ok().and_then(|bytes| {
                serde_json::from_slice::<RecoverableModuleSessionSnapshot>(&bytes).ok()
            })
        } else {
            None
        };

        if let Some(existing) = &mut snapshot {
            existing.normalize();
            let payload_dir = self.recoverable_module_session_payload_dir();
            if let Some(shared_state) = &existing.latest_shared_state {
                if !payload_dir
                    .join(&shared_state.cached_payload_name)
                    .is_file()
                {
                    existing.latest_shared_state = None;
                }
            }
            existing
                .recent_assets
                .retain(|asset| payload_dir.join(&asset.cached_payload_name).is_file());
        }

        let recoverable_policy = self.networking_recoverable_shared_chat_policy.as_ref();
        self.networking_recoverable_module_session = snapshot.filter(|item| {
            let Some(policy) = recoverable_policy else {
                return false;
            };
            item.session_id == policy.session_id
                && item.scope_module_id == policy.scope_module_id
                && !item.scope_module_id.trim().is_empty()
        });
        if had_snapshot && self.networking_recoverable_module_session.is_none() {
            self.discard_recoverable_module_session_snapshot();
        }
    }

    fn sync_recoverable_module_session_snapshot(&mut self) {
        let path = self.recoverable_module_session_path();
        let should_clear = matches!(
            (
                self.networking_recoverable_module_session.as_ref(),
                self.networking_recoverable_shared_chat_policy.as_ref()
            ),
            (Some(snapshot), Some(policy))
                if snapshot.session_id != policy.session_id
                    || snapshot.scope_module_id != policy.scope_module_id
        );
        if should_clear {
            self.networking_recoverable_module_session = None;
        }
        if let Some(snapshot) = &mut self.networking_recoverable_module_session {
            snapshot.normalize();
        }

        if let Some(snapshot) = &self.networking_recoverable_module_session {
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            match serde_json::to_vec_pretty(snapshot) {
                Ok(bytes) => {
                    if let Err(err) = fs::write(&path, bytes) {
                        if self.networking_status.is_none() {
                            self.networking_status = Some(format!(
                                "Could not save recoverable module session snapshot: {err}"
                            ));
                        }
                    }
                }
                Err(err) => {
                    if self.networking_status.is_none() {
                        self.networking_status = Some(format!(
                            "Could not serialize recoverable module session snapshot: {err}"
                        ));
                    }
                }
            }
        } else {
            let _ = fs::remove_file(&path);
        }
    }

    fn discard_recoverable_module_session_snapshot(&mut self) {
        self.networking_recoverable_module_session = None;
        let _ = fs::remove_file(self.recoverable_module_session_path());
        let _ = fs::remove_dir_all(self.recoverable_module_session_payload_dir());
    }

    fn active_recoverable_module_session_context(
        &self,
    ) -> Option<(String, String, String, String)> {
        let policy = &self.networking_shared_chat_policy;
        if !policy.session_active
            || policy.scope_kind != SharedChatScopeKind::Module
            || !self.shared_chat_is_local_host()
        {
            return None;
        }
        let module_id = policy.scope_module_id.trim().to_string();
        if module_id.is_empty() {
            return None;
        }
        Some((
            module_id,
            policy.scope_module_name.trim().to_string(),
            policy.session_id.trim().to_string(),
            policy.session_label.trim().to_string(),
        ))
    }

    fn ensure_recoverable_module_session_entry(
        &mut self,
    ) -> Option<(String, String, String, String)> {
        let context = self.active_recoverable_module_session_context()?;
        let (module_id, module_name, session_id, session_label) = context.clone();
        let needs_reset = self
            .networking_recoverable_module_session
            .as_ref()
            .is_none_or(|existing| {
                existing.session_id != session_id || existing.scope_module_id != module_id
            });
        if needs_reset {
            let _ = fs::remove_dir_all(self.recoverable_module_session_payload_dir());
            self.networking_recoverable_module_session = Some(RecoverableModuleSessionSnapshot {
                session_id: session_id.clone(),
                session_label: session_label.clone(),
                scope_module_id: module_id.clone(),
                scope_module_name: module_name.clone(),
                saved_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
                latest_shared_state: None,
                recent_assets: Vec::new(),
            });
        } else if let Some(existing) = &mut self.networking_recoverable_module_session {
            existing.scope_module_name = module_name.clone();
            existing.session_label = session_label.clone();
            existing.saved_at_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
        }
        Some(context)
    }

    fn remember_recoverable_module_shared_state(
        &mut self,
        module_id: &str,
        state: &ModuleBridgeSharedState,
        payload_text: &str,
    ) {
        let Some((scope_module_id, _, session_id, _)) =
            self.ensure_recoverable_module_session_entry()
        else {
            return;
        };
        if module_id.trim() != scope_module_id.trim()
            || state.session_id.trim() != session_id.trim()
        {
            return;
        }
        let payload_dir = self.recoverable_module_session_payload_dir();
        if fs::create_dir_all(&payload_dir).is_err() {
            return;
        }
        let cached_payload_name = format!(
            "{}__state_rev_{}.json",
            slugify_filename(module_id, "module"),
            state.session_revision.max(1)
        );
        let payload_path = payload_dir.join(&cached_payload_name);
        if fs::write(&payload_path, payload_text.as_bytes()).is_err() {
            return;
        }
        if let Some(snapshot) = &mut self.networking_recoverable_module_session {
            if let Some(previous) = &snapshot.latest_shared_state {
                if previous.cached_payload_name != cached_payload_name {
                    let _ = fs::remove_file(payload_dir.join(&previous.cached_payload_name));
                }
            }
            snapshot.latest_shared_state = Some(RecoverableModuleSharedStateSnapshot {
                summary: state.summary.clone(),
                session_revision: state.session_revision.max(1),
                cached_payload_name,
                updated_at_unix_ms: state
                    .updated_at_unix_ms
                    .max(Utc::now().timestamp_millis().max(0) as u64),
            });
            snapshot.saved_at_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
        }
        self.sync_recoverable_module_session_snapshot();
    }

    #[allow(dead_code)]
    fn remember_recoverable_module_asset(
        &mut self,
        kind: &str,
        label: &str,
        module_id: &str,
        summary: &str,
        file_name: &str,
        content_type: &str,
        bytes: &[u8],
        binary: bool,
    ) {
        let Some((scope_module_id, _, _, _)) = self.ensure_recoverable_module_session_entry()
        else {
            return;
        };
        if module_id.trim().is_empty() || module_id.trim() != scope_module_id.trim() {
            return;
        }
        let payload_dir = self.recoverable_module_session_payload_dir();
        if fs::create_dir_all(&payload_dir).is_err() {
            return;
        }
        let stored_at = Utc::now().timestamp_millis().max(0) as u64;
        let cached_payload_name = format!(
            "{}__{}__{}.{}",
            slugify_filename(module_id, "module"),
            slugify_filename(
                if label.trim().is_empty() {
                    kind.trim()
                } else {
                    label.trim()
                },
                "asset"
            ),
            stored_at,
            infer_transfer_extension(file_name, content_type, binary)
        );
        let payload_path = payload_dir.join(&cached_payload_name);
        if fs::write(&payload_path, bytes).is_err() {
            return;
        }
        if let Some(snapshot) = &mut self.networking_recoverable_module_session {
            snapshot.recent_assets.insert(
                0,
                RecoverableModuleSessionAssetSnapshot {
                    artifact_kind: kind.trim().to_string(),
                    label: label.trim().to_string(),
                    summary: summary.trim().to_string(),
                    file_name: file_name.trim().to_string(),
                    content_type: content_type.trim().to_string(),
                    byte_len: bytes.len() as u64,
                    binary,
                    cached_payload_name,
                    stored_at_unix_ms: stored_at,
                },
            );
            while snapshot.recent_assets.len() > 12 {
                if let Some(removed) = snapshot.recent_assets.pop() {
                    let _ = fs::remove_file(payload_dir.join(removed.cached_payload_name));
                }
            }
            snapshot.saved_at_unix_ms = stored_at;
        }
        self.sync_recoverable_module_session_snapshot();
    }

    fn restore_recoverable_module_shared_state_to_bridge(&mut self) -> Result<(), String> {
        let Some(recovery) = self.networking_recoverable_module_session.clone() else {
            return Err("No recoverable module session state is cached yet.".to_string());
        };
        let Some(shared_state) = recovery.latest_shared_state else {
            return Err(
                "No cached shared_state.json is available for this recovered session.".to_string(),
            );
        };
        let module = self
            .modules
            .iter()
            .find(|module| module.manifest.id == recovery.scope_module_id)
            .ok_or_else(|| "The recovered module is not currently available.".to_string())?;
        let payload_path = self
            .recoverable_module_session_payload_dir()
            .join(&shared_state.cached_payload_name);
        let bytes = fs::read(&payload_path).map_err(|err| {
            format!(
                "Could not read the cached shared state from {}: {err}",
                payload_path.display()
            )
        })?;
        let state: ModuleBridgeSharedState = serde_json::from_slice(&bytes)
            .map_err(|err| format!("Shared-state parse error: {err}"))?;
        write_bridge_shared_state(&module.folder, &state).map_err(|err| {
            format!(
                "Could not restore shared_state.json for {}: {err}",
                recovery.scope_module_name
            )
        })?;
        self.networking_status = Some(format!(
            "Restored the latest shared_state.json for {}.",
            if recovery.scope_module_name.trim().is_empty() {
                recovery.scope_module_id
            } else {
                recovery.scope_module_name
            }
        ));
        Ok(())
    }

    fn recovery_target_connection_ids(&self) -> Vec<String> {
        let snapshot = self.networking.snapshot().clone();
        let mut selected = snapshot
            .connected_peers
            .iter()
            .filter(|peer| {
                let key = if peer.device_id.trim().is_empty() {
                    peer.connection_id.clone()
                } else {
                    peer.device_id.clone()
                };
                self.networking_selected_devices.contains(&key)
            })
            .map(|peer| peer.connection_id.clone())
            .collect::<Vec<_>>();
        selected.sort();
        selected.dedup();
        if !selected.is_empty() {
            return selected;
        }
        self.shared_chat_connected_connection_ids()
    }

    fn replay_recoverable_module_shared_state(&mut self) -> Result<usize, String> {
        let Some(recovery) = self.networking_recoverable_module_session.clone() else {
            return Err("No recoverable module session state is cached yet.".to_string());
        };
        let Some(shared_state) = recovery.latest_shared_state else {
            return Err(
                "No cached shared_state.json is available for this session yet.".to_string(),
            );
        };
        let connection_ids = self.recovery_target_connection_ids();
        if connection_ids.is_empty() {
            return Err("Connect to one or more room peers first.".to_string());
        }
        let payload_path = self
            .recoverable_module_session_payload_dir()
            .join(&shared_state.cached_payload_name);
        let text = fs::read_to_string(&payload_path).map_err(|err| {
            format!(
                "Could not read the cached shared state from {}: {err}",
                payload_path.display()
            )
        })?;
        for connection_id in &connection_ids {
            self.networking.send_artifact(
                connection_id,
                "module_shared_state_json",
                if recovery.session_label.trim().is_empty() {
                    "Recovered module session state"
                } else {
                    recovery.session_label.trim()
                },
                Some(&recovery.scope_module_id),
                if shared_state.summary.trim().is_empty() {
                    "Recovered module session state"
                } else {
                    shared_state.summary.trim()
                },
                &format!(
                    "{}_shared_state_recovered.json",
                    slugify_filename(&recovery.scope_module_id, "module")
                ),
                &text,
            );
        }
        self.networking_status = Some(format!(
            "Re-shared the latest module session state to {} peer(s).",
            connection_ids.len()
        ));
        Ok(connection_ids.len())
    }

    fn replay_recoverable_module_assets(&mut self) -> Result<(usize, usize), String> {
        let Some(recovery) = self.networking_recoverable_module_session.clone() else {
            return Err("No recoverable module session assets are cached yet.".to_string());
        };
        if recovery.recent_assets.is_empty() {
            return Err("No recoverable module session assets are cached yet.".to_string());
        }
        let connection_ids = self.recovery_target_connection_ids();
        if connection_ids.is_empty() {
            return Err("Connect to one or more room peers first.".to_string());
        }
        let payload_dir = self.recoverable_module_session_payload_dir();
        let mut replayed = 0usize;
        for asset in &recovery.recent_assets {
            let payload_path = payload_dir.join(&asset.cached_payload_name);
            let Ok(bytes) = fs::read(&payload_path) else {
                continue;
            };
            for connection_id in &connection_ids {
                if asset.binary {
                    self.networking.send_artifact_bytes(
                        connection_id,
                        &asset.artifact_kind,
                        if asset.label.trim().is_empty() {
                            &asset.artifact_kind
                        } else {
                            &asset.label
                        },
                        Some(&recovery.scope_module_id),
                        &asset.summary,
                        if asset.file_name.trim().is_empty() {
                            &asset.cached_payload_name
                        } else {
                            &asset.file_name
                        },
                        &asset.content_type,
                        &bytes,
                    );
                } else if let Ok(text) = String::from_utf8(bytes.clone()) {
                    self.networking.send_artifact(
                        connection_id,
                        &asset.artifact_kind,
                        if asset.label.trim().is_empty() {
                            &asset.artifact_kind
                        } else {
                            &asset.label
                        },
                        Some(&recovery.scope_module_id),
                        &asset.summary,
                        if asset.file_name.trim().is_empty() {
                            &asset.cached_payload_name
                        } else {
                            &asset.file_name
                        },
                        &text,
                    );
                } else {
                    self.networking.send_artifact_bytes(
                        connection_id,
                        &asset.artifact_kind,
                        if asset.label.trim().is_empty() {
                            &asset.artifact_kind
                        } else {
                            &asset.label
                        },
                        Some(&recovery.scope_module_id),
                        &asset.summary,
                        if asset.file_name.trim().is_empty() {
                            &asset.cached_payload_name
                        } else {
                            &asset.file_name
                        },
                        &asset.content_type,
                        &bytes,
                    );
                }
            }
            replayed += 1;
        }
        self.networking_status = Some(format!(
            "Replayed {} recoverable module asset(s) to {} peer(s).",
            replayed,
            connection_ids.len()
        ));
        Ok((replayed, connection_ids.len()))
    }

    fn sync_recoverable_shared_chat_policy_snapshot(&mut self) {
        let next_policy = if self.networking_shared_chat_policy.session_active
            && self.shared_chat_is_local_host()
        {
            Some(Self::normalize_recoverable_shared_chat_policy(
                self.networking_shared_chat_policy.clone(),
            ))
        } else {
            None
        };
        let next_json = next_policy
            .as_ref()
            .and_then(|policy| serde_json::to_string(policy).ok());
        let changed = self.settings.network_recoverable_shared_chat_policy_json != next_json;
        self.networking_recoverable_shared_chat_policy = next_policy;
        if self.networking_recoverable_shared_chat_policy.is_none() {
            self.discard_recoverable_module_session_snapshot();
        } else {
            self.sync_recoverable_module_session_snapshot();
        }
        if !changed {
            return;
        }
        self.settings.network_recoverable_shared_chat_policy_json = next_json;
        if let Err(err) = save_settings(&self.settings, &self.base_path) {
            if self.networking_status.is_none() {
                self.networking_status = Some(format!(
                    "Could not save recoverable classroom room session: {err}"
                ));
            }
        }
    }

    fn discard_recoverable_shared_chat_policy(&mut self) {
        self.networking_recoverable_shared_chat_policy = None;
        self.discard_recoverable_module_session_snapshot();
        if self
            .settings
            .network_recoverable_shared_chat_policy_json
            .is_none()
        {
            return;
        }
        self.settings.network_recoverable_shared_chat_policy_json = None;
        if let Err(err) = save_settings(&self.settings, &self.base_path) {
            self.networking_status =
                Some(format!("Could not discard recoverable room session: {err}"));
        }
    }

    fn resume_recoverable_shared_chat_policy(&mut self) -> Result<(), String> {
        let Some(mut policy) = self.networking_recoverable_shared_chat_policy.clone() else {
            return Err("No recoverable classroom room session is saved yet.".to_string());
        };
        let snapshot = self.networking.snapshot().clone();
        if snapshot.device_id.trim().is_empty() {
            return Err("Local network identity is not ready yet.".to_string());
        }
        policy.updated_at_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
        policy.source_app = "chatty-edu".to_string();
        policy.host_device_id = snapshot.device_id.clone();
        policy.host_device_name = snapshot.device_name.clone();
        if policy.turn_mode == SharedChatTurnMode::Open {
            policy.turn_holder_device_id.clear();
            policy.turn_holder_device_name.clear();
            self.networking_turn_holder = None;
        } else {
            self.networking_turn_holder = Some(policy.turn_holder_device_id.clone());
        }
        self.networking_shared_chat_policy = policy;
        self.ensure_shared_chat_policy_defaults();
        self.networking_shared_chat_presence_key.clear();
        self.networking_shared_chat_presence_next_sync_at = Some(Instant::now());
        self.broadcast_shared_chat_policy_with_options(
            "Recovered the last saved classroom host session.",
            false,
            true,
            false,
        );
        let _ = self.restore_recoverable_module_shared_state_to_bridge();
        Ok(())
    }

    fn shared_chat_host_appears_offline(&self) -> bool {
        let host_id = self.networking_shared_chat_policy.host_device_id.trim();
        if host_id.is_empty() || self.shared_chat_is_local_host() {
            return false;
        }
        !self
            .networking
            .snapshot()
            .connected_peers
            .iter()
            .any(|peer| peer.device_id.trim() == host_id)
    }

    fn take_over_shared_chat_host(&mut self) -> Result<(), String> {
        let snapshot = self.networking.snapshot().clone();
        if snapshot.device_id.trim().is_empty() {
            return Err("Local network identity is not ready yet.".to_string());
        }
        let previous_host_id = self.networking_shared_chat_policy.host_device_id.clone();
        self.networking_shared_chat_policy.host_device_id = snapshot.device_id.clone();
        self.networking_shared_chat_policy.host_device_name = snapshot.device_name.clone();
        if self.networking_shared_chat_policy.turn_mode == SharedChatTurnMode::TalkingStick {
            let holder = self
                .networking_shared_chat_policy
                .turn_holder_device_id
                .trim()
                .to_string();
            if holder.is_empty() || holder == previous_host_id {
                self.networking_shared_chat_policy.turn_holder_device_id =
                    snapshot.device_id.clone();
                self.networking_shared_chat_policy.turn_holder_device_name =
                    snapshot.device_name.clone();
                self.networking_turn_holder = Some(snapshot.device_id);
            }
        }
        self.networking_shared_chat_presence_key.clear();
        self.networking_shared_chat_presence_next_sync_at = Some(Instant::now());
        self.broadcast_shared_chat_policy_with_options(
            "Local classroom device took over as room host.",
            false,
            true,
            false,
        );
        Ok(())
    }

    fn handoff_shared_chat_host_to_peer(
        &mut self,
        target_device_id: &str,
        target_device_name: &str,
    ) -> Result<(), String> {
        if !self.shared_chat_is_local_host() {
            return Err("Only the current host can hand off this room.".to_string());
        }
        let target_device_id = target_device_id.trim();
        if target_device_id.is_empty() {
            return Err("Pick a connected classroom device first.".to_string());
        }
        let snapshot = self.networking.snapshot().clone();
        if target_device_id == snapshot.device_id.trim() {
            return Err("That device is already the local host.".to_string());
        }
        self.networking_shared_chat_policy.host_device_id = target_device_id.to_string();
        self.networking_shared_chat_policy.host_device_name = target_device_name.trim().to_string();
        if self.networking_shared_chat_policy.turn_mode == SharedChatTurnMode::TalkingStick {
            let local_id = snapshot.device_id.trim();
            let holder = self
                .networking_shared_chat_policy
                .turn_holder_device_id
                .trim()
                .to_string();
            if holder.is_empty() || holder == local_id {
                self.networking_shared_chat_policy.turn_holder_device_id =
                    target_device_id.to_string();
                self.networking_shared_chat_policy.turn_holder_device_name =
                    target_device_name.trim().to_string();
                self.networking_turn_holder = Some(target_device_id.to_string());
            }
        }
        self.networking_shared_chat_presence_key.clear();
        self.broadcast_shared_chat_policy_with_options(
            &format!(
                "Host role handed to {}.",
                if target_device_name.trim().is_empty() {
                    target_device_id
                } else {
                    target_device_name.trim()
                }
            ),
            false,
            true,
            true,
        );
        Ok(())
    }

    fn network_trust_exports_dir(&self) -> PathBuf {
        self.base_path.join("network_exports").join("trusted_peers")
    }

    fn export_trusted_peer_list(&mut self) {
        if self.settings.network_trusted_devices.is_empty() {
            self.networking_status = Some(
                "Trust one or more classroom devices before exporting a trust list.".to_string(),
            );
            return;
        }

        let export_dir = self.network_trust_exports_dir();
        if let Err(err) = fs::create_dir_all(&export_dir) {
            self.networking_status = Some(format!(
                "Could not prepare the trust-list export folder: {err}"
            ));
            return;
        }

        let snapshot = self.networking.snapshot().clone();
        let export = NetworkPeerExchangeFile {
            version: "1".to_string(),
            source_app: "Chatty-EDU".to_string(),
            source_device_id: snapshot.device_id,
            source_device_name: snapshot.device_name,
            exported_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
            peers: self
                .settings
                .network_trusted_devices
                .iter()
                .map(|peer| NetworkPeerExchangeRecord {
                    device_id: peer.device_id.clone(),
                    device_name: peer.device_name.clone(),
                    alias: self
                        .settings
                        .network_device_aliases
                        .get(&peer.device_id)
                        .cloned()
                        .unwrap_or_default(),
                    group: self
                        .settings
                        .network_device_groups
                        .get(&peer.device_id)
                        .cloned()
                        .unwrap_or_default(),
                })
                .collect(),
        };

        let default_name = format!(
            "chatty_edu_trusted_devices_{}.json",
            export.exported_at_unix_ms
        );

        if let Some(path) = FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_directory(&export_dir)
            .set_file_name(&default_name)
            .save_file()
        {
            match serde_json::to_string_pretty(&export) {
                Ok(text) => match fs::write(&path, text) {
                    Ok(()) => {
                        self.networking_status = Some(format!(
                            "Exported {} trusted device(s) to {}.",
                            export.peers.len(),
                            path.display()
                        ));
                    }
                    Err(err) => {
                        self.networking_status =
                            Some(format!("Could not write the trust list: {err}"));
                    }
                },
                Err(err) => {
                    self.networking_status =
                        Some(format!("Could not serialize the trust list: {err}"));
                }
            }
        }
    }

    fn import_trusted_peer_list(&mut self) {
        let import_dir = self.network_trust_exports_dir();
        if let Err(err) = fs::create_dir_all(&import_dir) {
            self.networking_status = Some(format!(
                "Could not prepare the trust-list import folder: {err}"
            ));
            return;
        }

        let Some(path) = FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_directory(&import_dir)
            .pick_file()
        else {
            return;
        };

        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                self.networking_status = Some(format!(
                    "Could not read the trust list from {}: {}",
                    path.display(),
                    err
                ));
                return;
            }
        };

        let imported: NetworkPeerExchangeFile = match serde_json::from_str(&text) {
            Ok(imported) => imported,
            Err(err) => {
                self.networking_status = Some(format!(
                    "Could not parse the trust list from {}: {}",
                    path.display(),
                    err
                ));
                return;
            }
        };

        let local_device_id = self.networking.snapshot().device_id.clone();
        let blocked_ids: HashSet<String> = self
            .settings
            .network_blocked_devices
            .iter()
            .map(|peer| peer.device_id.clone())
            .collect();
        let mut added = 0usize;
        let mut refreshed = 0usize;
        let mut alias_added = 0usize;
        let mut group_added = 0usize;
        let mut skipped_self = 0usize;
        let mut skipped_blocked = 0usize;
        let mut skipped_empty = 0usize;

        for peer in imported.peers {
            let device_id = peer.device_id.trim().to_string();
            if device_id.is_empty() {
                skipped_empty += 1;
                continue;
            }
            if !local_device_id.trim().is_empty() && device_id == local_device_id {
                skipped_self += 1;
                continue;
            }
            if blocked_ids.contains(&device_id) {
                skipped_blocked += 1;
                continue;
            }

            let imported_name = if !peer.device_name.trim().is_empty() {
                peer.device_name.trim().to_string()
            } else if !peer.alias.trim().is_empty() {
                peer.alias.trim().to_string()
            } else {
                device_id.clone()
            };

            if let Some(existing) = self
                .settings
                .network_trusted_devices
                .iter_mut()
                .find(|entry| entry.device_id == device_id)
            {
                if existing.device_name.trim().is_empty() && !imported_name.trim().is_empty() {
                    existing.device_name = imported_name.clone();
                    refreshed += 1;
                }
            } else {
                self.settings
                    .network_trusted_devices
                    .push(crate::settings::StoredNetworkPeer {
                        device_id: device_id.clone(),
                        device_name: imported_name.clone(),
                    });
                added += 1;
            }

            if !peer.alias.trim().is_empty()
                && self
                    .settings
                    .network_device_aliases
                    .get(&device_id)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
            {
                self.settings
                    .network_device_aliases
                    .insert(device_id.clone(), peer.alias.trim().to_string());
                alias_added += 1;
            }

            if !peer.group.trim().is_empty()
                && self
                    .settings
                    .network_device_groups
                    .get(&device_id)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
            {
                self.settings
                    .network_device_groups
                    .insert(device_id.clone(), peer.group.trim().to_string());
                group_added += 1;
            }
        }

        let trusted_peers: Vec<TrustedPeer> = self
            .settings
            .network_trusted_devices
            .iter()
            .map(|peer| TrustedPeer {
                device_id: peer.device_id.clone(),
                device_name: peer.device_name.clone(),
                ..Default::default()
            })
            .collect();
        self.networking.replace_trusted_peers(&trusted_peers);
        self.persist_network_settings();

        let imported_count = added + refreshed;
        let mut message = format!(
            "Imported trust list from {}. Added {}, refreshed {}, aliases {}, groups {}, skipped self {}, blocked {}, empty {}.",
            path.display(),
            added,
            refreshed,
            alias_added,
            group_added,
            skipped_self,
            skipped_blocked,
            skipped_empty
        );
        if imported_count == 0 && alias_added == 0 && group_added == 0 {
            message.push_str(" Nothing new was applied.");
        }
        self.networking_status = Some(message);
    }

    fn export_blocked_peer_list(&mut self) {
        if self.settings.network_blocked_devices.is_empty() {
            self.networking_status = Some(
                "Block one or more classroom devices before exporting a blocked list.".to_string(),
            );
            return;
        }

        let export_dir = self.network_trust_exports_dir();
        if let Err(err) = fs::create_dir_all(&export_dir) {
            self.networking_status = Some(format!(
                "Could not prepare the blocked-list export folder: {err}"
            ));
            return;
        }

        let snapshot = self.networking.snapshot().clone();
        let export = NetworkPeerExchangeFile {
            version: "1".to_string(),
            source_app: "Chatty-EDU".to_string(),
            source_device_id: snapshot.device_id,
            source_device_name: snapshot.device_name,
            exported_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
            peers: self
                .settings
                .network_blocked_devices
                .iter()
                .map(|peer| NetworkPeerExchangeRecord {
                    device_id: peer.device_id.clone(),
                    device_name: peer.device_name.clone(),
                    alias: self
                        .settings
                        .network_device_aliases
                        .get(&peer.device_id)
                        .cloned()
                        .unwrap_or_default(),
                    group: self
                        .settings
                        .network_device_groups
                        .get(&peer.device_id)
                        .cloned()
                        .unwrap_or_default(),
                })
                .collect(),
        };

        let default_name = format!(
            "chatty_edu_blocked_devices_{}.json",
            export.exported_at_unix_ms
        );

        if let Some(path) = FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_directory(&export_dir)
            .set_file_name(&default_name)
            .save_file()
        {
            match serde_json::to_string_pretty(&export) {
                Ok(text) => match fs::write(&path, text) {
                    Ok(()) => {
                        self.networking_status = Some(format!(
                            "Exported {} blocked device(s) to {}.",
                            export.peers.len(),
                            path.display()
                        ));
                    }
                    Err(err) => {
                        self.networking_status =
                            Some(format!("Could not write the blocked list: {err}"));
                    }
                },
                Err(err) => {
                    self.networking_status =
                        Some(format!("Could not serialize the blocked list: {err}"));
                }
            }
        }
    }

    fn import_blocked_peer_list(&mut self) {
        let import_dir = self.network_trust_exports_dir();
        if let Err(err) = fs::create_dir_all(&import_dir) {
            self.networking_status = Some(format!(
                "Could not prepare the blocked-list import folder: {err}"
            ));
            return;
        }

        let Some(path) = FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_directory(&import_dir)
            .pick_file()
        else {
            return;
        };

        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                self.networking_status = Some(format!(
                    "Could not read the blocked list from {}: {}",
                    path.display(),
                    err
                ));
                return;
            }
        };

        let imported: NetworkPeerExchangeFile = match serde_json::from_str(&text) {
            Ok(imported) => imported,
            Err(err) => {
                self.networking_status = Some(format!(
                    "Could not parse the blocked list from {}: {}",
                    path.display(),
                    err
                ));
                return;
            }
        };

        let local_device_id = self.networking.snapshot().device_id.clone();
        let mut added = 0usize;
        let mut refreshed = 0usize;
        let mut alias_added = 0usize;
        let mut group_added = 0usize;
        let mut skipped_self = 0usize;
        let mut skipped_empty = 0usize;
        let mut trust_removed = 0usize;

        for peer in imported.peers {
            let device_id = peer.device_id.trim().to_string();
            if device_id.is_empty() {
                skipped_empty += 1;
                continue;
            }
            if !local_device_id.trim().is_empty() && device_id == local_device_id {
                skipped_self += 1;
                continue;
            }

            let imported_name = if !peer.device_name.trim().is_empty() {
                peer.device_name.trim().to_string()
            } else if !peer.alias.trim().is_empty() {
                peer.alias.trim().to_string()
            } else {
                device_id.clone()
            };

            let trusted_before = self.settings.network_trusted_devices.len();
            self.settings
                .network_trusted_devices
                .retain(|entry| entry.device_id != device_id);
            trust_removed +=
                trusted_before.saturating_sub(self.settings.network_trusted_devices.len());

            if let Some(existing) = self
                .settings
                .network_blocked_devices
                .iter_mut()
                .find(|entry| entry.device_id == device_id)
            {
                if existing.device_name.trim().is_empty() && !imported_name.trim().is_empty() {
                    existing.device_name = imported_name.clone();
                    refreshed += 1;
                }
            } else {
                self.settings
                    .network_blocked_devices
                    .push(crate::settings::StoredNetworkPeer {
                        device_id: device_id.clone(),
                        device_name: imported_name.clone(),
                    });
                added += 1;
            }

            if !peer.alias.trim().is_empty()
                && self
                    .settings
                    .network_device_aliases
                    .get(&device_id)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
            {
                self.settings
                    .network_device_aliases
                    .insert(device_id.clone(), peer.alias.trim().to_string());
                alias_added += 1;
            }

            if !peer.group.trim().is_empty()
                && self
                    .settings
                    .network_device_groups
                    .get(&device_id)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
            {
                self.settings
                    .network_device_groups
                    .insert(device_id.clone(), peer.group.trim().to_string());
                group_added += 1;
            }
        }

        let trusted_peers: Vec<TrustedPeer> = self
            .settings
            .network_trusted_devices
            .iter()
            .map(|peer| TrustedPeer {
                device_id: peer.device_id.clone(),
                device_name: peer.device_name.clone(),
                ..Default::default()
            })
            .collect();
        let blocked_peers: Vec<BlockedPeer> = self
            .settings
            .network_blocked_devices
            .iter()
            .map(|peer| BlockedPeer {
                device_id: peer.device_id.clone(),
                device_name: peer.device_name.clone(),
                ..Default::default()
            })
            .collect();
        self.networking.replace_trusted_peers(&trusted_peers);
        self.networking.replace_blocked_peers(&blocked_peers);
        self.persist_network_settings();

        let imported_count = added + refreshed;
        let mut message = format!(
            "Imported blocked list from {}. Added {}, refreshed {}, trust removed {}, aliases {}, groups {}, skipped self {}, empty {}.",
            path.display(),
            added,
            refreshed,
            trust_removed,
            alias_added,
            group_added,
            skipped_self,
            skipped_empty
        );
        if imported_count == 0 && alias_added == 0 && group_added == 0 && trust_removed == 0 {
            message.push_str(" Nothing new was applied.");
        }
        self.networking_status = Some(message);
    }

    fn network_display_name(&self, device_id: &str, fallback: &str) -> String {
        self.settings
            .network_device_aliases
            .get(device_id)
            .filter(|alias| !alias.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    }

    fn network_group_label(&self, device_id: &str) -> Option<String> {
        self.settings
            .network_device_groups
            .get(device_id)
            .map(|group| group.trim().to_string())
            .filter(|group| !group.is_empty())
    }

    fn network_is_trusted(&self, device_id: &str) -> bool {
        !device_id.trim().is_empty()
            && self
                .settings
                .network_trusted_devices
                .iter()
                .any(|peer| peer.device_id == device_id)
    }

    fn trust_network_peer(&mut self, device_id: &str, fallback: &str) {
        if device_id.trim().is_empty() {
            self.networking_status = Some(
                "This device has not shared a stable ID yet, so it cannot be trusted.".to_string(),
            );
            return;
        }

        let display_name = self.network_display_name(device_id, fallback);
        self.settings
            .network_trusted_devices
            .retain(|entry| entry.device_id != device_id);
        self.settings
            .network_blocked_devices
            .retain(|entry| entry.device_id != device_id);
        self.settings
            .network_trusted_devices
            .push(crate::settings::StoredNetworkPeer {
                device_id: device_id.to_string(),
                device_name: display_name.clone(),
            });
        self.networking.trust_peer(device_id, &display_name);
        self.persist_network_settings();
        self.networking_status = Some(format!(
            "Trusted {}. Future classroom connections will be approved automatically.",
            display_name
        ));
    }

    fn untrust_network_peer(&mut self, device_id: &str, fallback: &str) {
        if device_id.trim().is_empty() {
            return;
        }

        self.settings
            .network_trusted_devices
            .retain(|entry| entry.device_id != device_id);
        self.networking.untrust_peer(device_id);
        self.persist_network_settings();
        self.networking_status = Some(format!(
            "Removed {} from trusted devices.",
            self.network_display_name(device_id, fallback)
        ));
    }

    fn block_network_peer(&mut self, device_id: &str, fallback: &str) {
        if device_id.trim().is_empty() {
            return;
        }

        let display_name = self.network_display_name(device_id, fallback);
        self.networking.block_peer(device_id, &display_name);
        self.settings
            .network_trusted_devices
            .retain(|entry| entry.device_id != device_id);
        self.settings
            .network_blocked_devices
            .retain(|entry| entry.device_id != device_id);
        self.settings
            .network_blocked_devices
            .push(crate::settings::StoredNetworkPeer {
                device_id: device_id.to_string(),
                device_name: display_name.clone(),
            });
        self.persist_network_settings();
        self.networking_status = Some(format!("{display_name} is now blocked."));
    }

    fn unblock_network_peer(&mut self, device_id: &str, fallback: &str) {
        if device_id.trim().is_empty() {
            return;
        }

        let display_name = self.network_display_name(device_id, fallback);
        self.networking.unblock_peer(device_id);
        self.settings
            .network_blocked_devices
            .retain(|entry| entry.device_id != device_id);
        self.persist_network_settings();
        self.networking_status = Some(format!("Unblocked {}.", display_name));
    }

    fn begin_network_alias_edit(&mut self, device_id: &str, fallback: &str) {
        if device_id.trim().is_empty() {
            self.networking_status = Some(
                "This device has not shared a stable ID yet, so it cannot be renamed.".to_string(),
            );
            return;
        }

        self.networking_alias_edit_device = Some(device_id.to_string());
        self.networking_alias_input = self.network_display_name(device_id, fallback);
    }

    fn cancel_network_alias_edit(&mut self) {
        self.networking_alias_edit_device = None;
        self.networking_alias_input.clear();
    }

    fn save_network_alias_edit(&mut self, device_id: &str, fallback: &str) {
        if device_id.trim().is_empty() {
            self.cancel_network_alias_edit();
            return;
        }

        let trimmed = self.networking_alias_input.trim().to_string();
        if trimmed.is_empty() || trimmed == fallback.trim() {
            self.settings.network_device_aliases.remove(device_id);
            self.networking_status =
                Some(format!("Cleared the custom name for {}.", fallback.trim()));
        } else {
            self.settings
                .network_device_aliases
                .insert(device_id.to_string(), trimmed.clone());
            self.networking_status = Some(format!("Saved \"{trimmed}\" for {}.", fallback.trim()));
        }
        self.persist_network_settings();
        self.cancel_network_alias_edit();
    }

    fn begin_network_group_edit(&mut self, device_id: &str) {
        if device_id.trim().is_empty() {
            self.networking_status = Some(
                "This device has not shared a stable ID yet, so a group cannot be saved."
                    .to_string(),
            );
            return;
        }

        self.networking_group_edit_device = Some(device_id.to_string());
        self.networking_group_input = self.network_group_label(device_id).unwrap_or_default();
    }

    fn cancel_network_group_edit(&mut self) {
        self.networking_group_edit_device = None;
        self.networking_group_input.clear();
    }

    fn save_network_group_edit(&mut self, device_id: &str, fallback: &str) {
        if device_id.trim().is_empty() {
            self.cancel_network_group_edit();
            return;
        }

        let trimmed = self.networking_group_input.trim().to_string();
        if trimmed.is_empty() {
            self.settings.network_device_groups.remove(device_id);
            self.networking_status =
                Some(format!("Cleared the group label for {}.", fallback.trim()));
        } else {
            self.settings
                .network_device_groups
                .insert(device_id.to_string(), trimmed.clone());
            self.networking_status = Some(format!(
                "Saved group \"{trimmed}\" for {}.",
                fallback.trim()
            ));
        }
        self.persist_network_settings();
        self.cancel_network_group_edit();
    }

    fn focus_networking_section(&mut self, section: NetworkingFocusSection) {
        self.networking_focus_section = Some(section);
        self.networking_focus_pending = Some(section);
        self.networking_focus_flash_until = Some(Instant::now() + Duration::from_secs(6));
    }

    fn current_role(&self) -> &str {
        if self.teacher_unlocked {
            "teacher"
        } else {
            "student"
        }
    }

    fn try_unlock_teacher(&mut self) {
        if self.settings.teacher_pin == self.teacher_pin_input.trim() {
            self.teacher_unlocked = true;
            self.teacher_pin_status = Some("Teacher view unlocked".to_string());
            self.pulse_ecg(26.0, "Teacher view unlocked.");
        } else {
            self.teacher_pin_status = Some("Incorrect PIN".to_string());
        }
        self.teacher_pin_input.clear();
    }

    fn lock_teacher(&mut self) {
        self.teacher_unlocked = false;
        self.close_tabs_by_key_prefix("bookkeeper");
        self.teacher_pin_status = Some("Teacher view locked".to_string());
        self.pulse_ecg(12.0, "Teacher view locked.");
    }

    fn change_teacher_pin(&mut self) {
        if !self.teacher_unlocked {
            self.teacher_pin_status = Some("Unlock first to change PIN".to_string());
            return;
        }
        if self.teacher_pin_new.trim().is_empty() {
            self.teacher_pin_status = Some("PIN cannot be empty".to_string());
            return;
        }
        if self.teacher_pin_new != self.teacher_pin_confirm {
            self.teacher_pin_status = Some("PINs did not match".to_string());
            return;
        }
        self.settings.teacher_pin = self.teacher_pin_new.trim().to_string();
        self.teacher_pin_new.clear();
        self.teacher_pin_confirm.clear();
        match save_settings(&self.settings, &self.base_path) {
            Ok(_) => {
                self.teacher_pin_status = Some("PIN updated".to_string());
                self.pulse_ecg(20.0, "Teacher PIN updated.");
            }
            Err(e) => self.teacher_pin_status = Some(format!("Failed to save PIN: {e}")),
        }
    }

    fn update_secret_question(&mut self) {
        if !self.teacher_unlocked {
            self.teacher_pin_status = Some("Unlock first to change secret question".to_string());
            return;
        }
        if self.teacher_secret_question_input.trim().is_empty()
            || self.teacher_secret_answer_input.trim().is_empty()
        {
            self.teacher_pin_status =
                Some("Secret question and answer cannot be empty".to_string());
            return;
        }
        self.settings.teacher_secret_question =
            self.teacher_secret_question_input.trim().to_string();
        self.settings.teacher_secret_answer = self.teacher_secret_answer_input.trim().to_string();
        match save_settings(&self.settings, &self.base_path) {
            Ok(_) => {
                self.teacher_pin_status = Some("Secret question updated".to_string());
                self.pulse_ecg(20.0, "Teacher recovery question updated.");
            }
            Err(e) => self.teacher_pin_status = Some(format!("Failed to save secret: {e}")),
        }
        self.teacher_secret_answer_input.clear();
    }

    fn open_teacher_dashboard(&mut self) {
        if let Some(module) = self
            .modules
            .iter()
            .find(|m| m.manifest.id == "homework_dashboard")
            .cloned()
        {
            self.open_module_tab(&module);
        }
    }

    fn open_revision_workspace(&mut self) {
        if let Some(module) = self
            .modules
            .iter()
            .find(|m| m.manifest.id == "homework_assignments")
            .cloned()
        {
            self.open_module_tab(&module);
        }
    }

    fn switch_theme(&mut self, name: &str, ctx: &Context) {
        self.theme = load_theme(&self.base_path, Some(name));
        apply_theme(&self.theme, ctx);
        self.settings.ui.last_theme = Some(self.theme.name.clone());
        let _ = save_theme(&self.base_path, &self.theme);
        let _ = save_settings(&self.settings, &self.base_path);
        self.pulse_ecg(10.0, "Updated the current theme.");
    }

    fn unique_assignments_by_id<'a>(pack: &'a HomeworkPack) -> Vec<&'a HomeworkAssignment> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();

        for assignment in &pack.assignments {
            let key = if assignment.id.trim().is_empty() {
                format!("title::{}", assignment.title.trim().to_ascii_lowercase())
            } else {
                assignment.id.trim().to_ascii_lowercase()
            };
            if seen.insert(key) {
                out.push(assignment);
            }
        }

        out
    }

    fn refresh_homework_question_index(&mut self) {
        self.homework_question_index = self
            .current_pack
            .as_ref()
            .map(Self::extract_homework_question_index)
            .unwrap_or_default();

        if let Some(pack) = self.current_pack.as_ref() {
            let visible_assignments = Self::unique_assignments_by_id(pack);
            let selected_is_valid = self.selected_assignment.as_ref().is_some_and(|id| {
                visible_assignments
                    .iter()
                    .any(|assignment| &assignment.id == id)
            });
            if !selected_is_valid {
                self.selected_assignment = visible_assignments.first().map(|a| a.id.clone());
            }
        } else {
            self.selected_assignment = None;
        }
    }

    fn extract_homework_question_index(pack: &HomeworkPack) -> Vec<HomeworkQuestionIntercept> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();

        for assignment in Self::unique_assignments_by_id(pack) {
            let mut sources = vec![assignment.instructions_md.clone()];
            if let Some(printable) = assignment.student_printable_md.as_deref() {
                let cleaned = Self::clean_markdown_fences(printable);
                if !cleaned.is_empty()
                    && Self::normalize_compare_text(&cleaned)
                        != Self::normalize_compare_text(&assignment.instructions_md)
                {
                    sources.push(cleaned);
                }
            }

            for source in sources {
                for (question_number, question_text) in Self::extract_question_candidates(&source) {
                    let normalized_question = Self::normalize_homework_match_text(&question_text);
                    if normalized_question.is_empty() {
                        continue;
                    }

                    let dedupe_key = format!(
                        "{}::{}",
                        assignment.id.trim().to_ascii_lowercase(),
                        normalized_question
                    );
                    if !seen.insert(dedupe_key) {
                        continue;
                    }

                    let tokens = Self::homework_match_tokens(&normalized_question);
                    if tokens.is_empty() {
                        continue;
                    }

                    out.push(HomeworkQuestionIntercept {
                        assignment_id: assignment.id.clone(),
                        question_number,
                        question_text,
                        normalized_question: normalized_question.clone(),
                        keyword_tokens: Self::homework_keyword_tokens(&tokens),
                        number_tokens: Self::homework_number_tokens(&tokens),
                        signature_phrases: Self::homework_signature_phrases(
                            &normalized_question,
                            &tokens,
                        ),
                        tokens,
                    });
                }
            }
        }

        out
    }

    fn resync_homework(&mut self) {
        self.current_pack = find_latest_pack(&self.base_path)
            .ok()
            .flatten()
            .map(|(_p, pack)| pack);
        self.refresh_received_homework_inbox();
        self.submissions = load_submission_summaries(&self.base_path).unwrap_or_default();
        self.refresh_homework_question_index();
        self.pulse_ecg(28.0, "Rescanned homework packs and submissions.");
    }

    fn selected_network_connection_ids(&self) -> Vec<String> {
        self.networking
            .snapshot()
            .connected_peers
            .iter()
            .filter_map(|peer| {
                let key = if peer.device_id.trim().is_empty() {
                    peer.connection_id.clone()
                } else {
                    peer.device_id.clone()
                };
                self.networking_selected_devices
                    .contains(&key)
                    .then_some(peer.connection_id.clone())
            })
            .collect()
    }

    fn shared_chat_connected_connection_ids(&self) -> Vec<String> {
        self.networking
            .snapshot()
            .connected_peers
            .iter()
            .map(|peer| peer.connection_id.clone())
            .collect()
    }

    fn shared_chat_connected_peer_keys(&self) -> HashSet<String> {
        self.networking
            .snapshot()
            .connected_peers
            .iter()
            .map(|peer| {
                if peer.device_id.trim().is_empty() {
                    peer.connection_id.clone()
                } else {
                    peer.device_id.clone()
                }
            })
            .collect()
    }

    fn ensure_shared_chat_policy_defaults(&mut self) {
        let snapshot = self.networking.snapshot().clone();
        if self.networking_shared_chat_policy.version.trim().is_empty() {
            self.networking_shared_chat_policy.version = "1".to_string();
        }
        if self.networking_shared_chat_policy.label.trim().is_empty() {
            self.networking_shared_chat_policy.label = "Classroom room".to_string();
        }
        if self
            .networking_shared_chat_policy
            .source_app
            .trim()
            .is_empty()
        {
            self.networking_shared_chat_policy.source_app = "chatty-edu".to_string();
        }
        if self
            .networking_shared_chat_policy
            .host_device_id
            .trim()
            .is_empty()
        {
            self.networking_shared_chat_policy.host_device_id = snapshot.device_id.clone();
        }
        if self
            .networking_shared_chat_policy
            .host_device_name
            .trim()
            .is_empty()
        {
            self.networking_shared_chat_policy.host_device_name = snapshot.device_name.clone();
        }
        if self.networking_shared_chat_policy.scope_kind == SharedChatScopeKind::Module
            && self
                .networking_shared_chat_policy
                .scope_module_id
                .trim()
                .is_empty()
        {
            self.networking_shared_chat_policy.scope_kind = SharedChatScopeKind::General;
            self.networking_shared_chat_policy.scope_module_name.clear();
            self.networking_shared_chat_policy.scope_multiplayer = false;
        }
        if self.networking_shared_chat_policy.scope_kind == SharedChatScopeKind::General {
            self.networking_shared_chat_policy.session_active = false;
            self.networking_shared_chat_policy.session_id.clear();
            self.networking_shared_chat_policy.session_revision = 0;
            self.networking_shared_chat_policy.session_label.clear();
            self.networking_shared_chat_policy.host_authoritative = false;
        } else {
            self.networking_shared_chat_policy.host_authoritative =
                self.shared_chat_scoped_module_host_authoritative();
            if self.networking_shared_chat_policy.session_active
                && self
                    .networking_shared_chat_policy
                    .session_id
                    .trim()
                    .is_empty()
            {
                self.networking_shared_chat_policy.session_active = false;
                self.networking_shared_chat_policy.session_revision = 0;
            }
            if self.networking_shared_chat_policy.session_active
                && self
                    .networking_shared_chat_policy
                    .session_label
                    .trim()
                    .is_empty()
                && !self
                    .networking_shared_chat_policy
                    .scope_module_name
                    .trim()
                    .is_empty()
            {
                self.networking_shared_chat_policy.session_label = format!(
                    "{} room session",
                    self.networking_shared_chat_policy.scope_module_name.trim()
                );
            }
        }
        if self.networking_shared_chat_policy.turn_mode == SharedChatTurnMode::Open {
            self.networking_shared_chat_policy
                .turn_holder_device_id
                .clear();
            self.networking_shared_chat_policy
                .turn_holder_device_name
                .clear();
            self.networking_turn_holder = None;
        } else if self
            .networking_shared_chat_policy
            .turn_holder_device_id
            .trim()
            .is_empty()
        {
            self.networking_shared_chat_policy.turn_holder_device_id = snapshot.device_id.clone();
            self.networking_shared_chat_policy.turn_holder_device_name = snapshot.device_name;
            self.networking_turn_holder = Some(
                self.networking_shared_chat_policy
                    .turn_holder_device_id
                    .clone(),
            );
        } else {
            self.networking_turn_holder = Some(
                self.networking_shared_chat_policy
                    .turn_holder_device_id
                    .clone(),
            );
        }
    }

    fn shared_chat_capable_modules(&self) -> Vec<(String, String, bool)> {
        self.modules
            .iter()
            .filter_map(|module| {
                let caps = module.manifest.network_capabilities.as_ref()?;
                let room_aware = caps.has(ModuleNetworkFeature::RoomAware);
                let multiplayer = caps.has(ModuleNetworkFeature::Multiplayer);
                if !room_aware && !multiplayer {
                    return None;
                }
                Some((
                    module.manifest.id.clone(),
                    module.manifest.title.clone(),
                    multiplayer,
                ))
            })
            .collect()
    }

    fn shared_chat_scoped_module(&self) -> Option<&LoadedModule> {
        if self.networking_shared_chat_policy.scope_kind != SharedChatScopeKind::Module {
            return None;
        }
        let scoped_id = self.networking_shared_chat_policy.scope_module_id.trim();
        if scoped_id.is_empty() {
            return None;
        }
        self.modules
            .iter()
            .find(|module| module.manifest.id.trim() == scoped_id)
    }

    fn module_by_id(&self, module_id: &str) -> Option<LoadedModule> {
        let module_id = module_id.trim();
        if module_id.is_empty() {
            return None;
        }
        self.modules
            .iter()
            .find(|module| module.manifest.id.trim() == module_id)
            .cloned()
    }

    fn shared_chat_scoped_module_host_authoritative(&self) -> bool {
        self.shared_chat_scoped_module()
            .and_then(|module| module.manifest.network_capabilities.as_ref())
            .map(|caps| caps.has(ModuleNetworkFeature::HostAuthoritative))
            .unwrap_or(false)
    }

    fn shared_chat_scope_label(&self) -> String {
        match self.networking_shared_chat_policy.scope_kind {
            SharedChatScopeKind::General => "General room".to_string(),
            SharedChatScopeKind::Module => {
                let name = self.networking_shared_chat_policy.scope_module_name.trim();
                if name.is_empty() {
                    "Module room".to_string()
                } else if self.networking_shared_chat_policy.scope_multiplayer {
                    format!("{name} (multiplayer)")
                } else {
                    format!("{name} (module)")
                }
            }
        }
    }

    fn shared_chat_scope_matches_module(&self, module_id: &str) -> bool {
        self.networking_shared_chat_policy.scope_kind == SharedChatScopeKind::Module
            && self.networking_shared_chat_policy.scope_module_id.trim() == module_id.trim()
    }

    fn set_shared_chat_scope_general(&mut self) {
        self.networking_shared_chat_policy.scope_kind = SharedChatScopeKind::General;
        self.networking_shared_chat_policy.scope_module_id.clear();
        self.networking_shared_chat_policy.scope_module_name.clear();
        self.networking_shared_chat_policy.scope_multiplayer = false;
        self.networking_shared_chat_policy.session_active = false;
        self.networking_shared_chat_policy.session_id.clear();
        self.networking_shared_chat_policy.session_revision = 0;
        self.networking_shared_chat_policy.session_label.clear();
        self.networking_shared_chat_policy.host_authoritative = false;
    }

    fn set_shared_chat_scope_module(
        &mut self,
        module_id: impl Into<String>,
        module_name: impl Into<String>,
        multiplayer: bool,
    ) {
        self.networking_shared_chat_policy.scope_kind = SharedChatScopeKind::Module;
        self.networking_shared_chat_policy.scope_module_id = module_id.into().trim().to_string();
        self.networking_shared_chat_policy.scope_module_name =
            module_name.into().trim().to_string();
        self.networking_shared_chat_policy.scope_multiplayer = multiplayer;
        if self
            .networking_shared_chat_policy
            .scope_module_id
            .trim()
            .is_empty()
        {
            self.set_shared_chat_scope_general();
        } else {
            self.networking_shared_chat_policy.host_authoritative =
                self.shared_chat_scoped_module_host_authoritative();
            if self
                .networking_shared_chat_policy
                .session_label
                .trim()
                .is_empty()
                && !self
                    .networking_shared_chat_policy
                    .scope_module_name
                    .trim()
                    .is_empty()
            {
                self.networking_shared_chat_policy.session_label = format!(
                    "{} room session",
                    self.networking_shared_chat_policy.scope_module_name.trim()
                );
            }
        }
    }

    fn shared_chat_session_summary(&self) -> Option<String> {
        if !self.networking_shared_chat_policy.session_active {
            return None;
        }
        let label = if self
            .networking_shared_chat_policy
            .session_label
            .trim()
            .is_empty()
        {
            self.shared_chat_scope_label()
        } else {
            self.networking_shared_chat_policy
                .session_label
                .trim()
                .to_string()
        };
        Some(format!(
            "{} | revision {}{}",
            label,
            self.networking_shared_chat_policy.session_revision.max(1),
            if self.networking_shared_chat_policy.host_authoritative {
                " | host-authoritative"
            } else {
                ""
            }
        ))
    }

    fn begin_shared_chat_module_session(&mut self) -> Option<String> {
        if self.networking_shared_chat_policy.scope_kind != SharedChatScopeKind::Module {
            return None;
        }
        let scoped_module_id = self.networking_shared_chat_policy.scope_module_id.clone();
        self.reset_module_shared_session(&scoped_module_id);
        self.discard_recoverable_module_session_snapshot();
        let module_name = if self
            .networking_shared_chat_policy
            .scope_module_name
            .trim()
            .is_empty()
        {
            self.networking_shared_chat_policy.scope_module_id.clone()
        } else {
            self.networking_shared_chat_policy.scope_module_name.clone()
        };
        self.networking_shared_chat_policy.session_active = true;
        self.networking_shared_chat_policy.session_id = format!(
            "room-{}-{}",
            slugify_filename(
                &self.networking_shared_chat_policy.scope_module_id,
                "module"
            ),
            Utc::now().timestamp_millis().max(0) as u64
        );
        self.networking_shared_chat_policy.session_revision = 0;
        self.networking_shared_chat_policy.session_label = format!("{} room session", module_name);
        self.networking_shared_chat_policy.host_authoritative =
            self.shared_chat_scoped_module_host_authoritative();
        if self.networking_shared_chat_policy.scope_multiplayer
            && self.networking_shared_chat_policy.turn_mode == SharedChatTurnMode::Open
        {
            let local = self.networking.snapshot().clone();
            self.networking_shared_chat_policy.turn_mode = SharedChatTurnMode::TalkingStick;
            self.networking_shared_chat_policy.turn_holder_device_id = local.device_id.clone();
            self.networking_shared_chat_policy.turn_holder_device_name = local.device_name.clone();
            self.networking_turn_holder = Some(local.device_id);
        }
        Some(module_name)
    }

    fn end_shared_chat_module_session(&mut self) {
        let scoped_module_id = self.networking_shared_chat_policy.scope_module_id.clone();
        self.reset_module_shared_session(&scoped_module_id);
        self.discard_recoverable_module_session_snapshot();
        self.networking_shared_chat_policy.session_active = false;
        self.networking_shared_chat_policy.session_id.clear();
        self.networking_shared_chat_policy.session_revision = 0;
        self.networking_shared_chat_policy.session_label.clear();
        self.networking_shared_chat_policy.host_authoritative = false;
    }

    fn shared_chat_teacher_override_active(&self) -> bool {
        self.networking_shared_chat_policy.teacher_override && self.teacher_unlocked
    }

    fn shared_chat_is_local_host(&self) -> bool {
        let local_id = self.networking.snapshot().device_id.as_str();
        self.networking_shared_chat_policy
            .host_device_id
            .trim()
            .is_empty()
            || self.networking_shared_chat_policy.host_device_id == local_id
    }

    fn shared_chat_turn_holder_label(&self) -> String {
        if self
            .networking_shared_chat_policy
            .turn_holder_device_name
            .trim()
            .is_empty()
        {
            if self
                .networking_shared_chat_policy
                .turn_holder_device_id
                .trim()
                .is_empty()
            {
                "unassigned".to_string()
            } else {
                self.networking_shared_chat_policy
                    .turn_holder_device_id
                    .clone()
            }
        } else {
            self.networking_shared_chat_policy
                .turn_holder_device_name
                .clone()
        }
    }

    fn shared_chat_policy_summary(&self) -> String {
        let mut parts = vec![
            self.networking_shared_chat_policy
                .turn_mode
                .label()
                .to_string(),
            format!("AI {}", self.networking_shared_chat_policy.ai_mode.label()),
            self.shared_chat_scope_label(),
        ];
        if self.networking_shared_chat_policy.turn_mode == SharedChatTurnMode::TalkingStick {
            parts.push(format!("Stick {}", self.shared_chat_turn_holder_label()));
        }
        if let Some(session) = self.shared_chat_session_summary() {
            parts.push(session);
        }
        if self.networking_shared_chat_policy.teacher_override {
            parts.push("Teacher override".to_string());
        }
        if !self
            .networking_shared_chat_policy
            .host_device_name
            .trim()
            .is_empty()
        {
            parts.push(format!(
                "Host {}",
                self.networking_shared_chat_policy.host_device_name.trim()
            ));
        }
        parts.join(" | ")
    }

    fn derive_module_host_activity_presence(
        &self,
        module_id: &str,
        module_dir: &Path,
    ) -> (String, String, u64, String) {
        let Ok(Some(status)) = read_bridge_status(module_dir) else {
            return (String::new(), String::new(), 0, String::new());
        };
        if !status.module_id.trim().is_empty() && status.module_id.trim() != module_id.trim() {
            return (String::new(), String::new(), 0, String::new());
        }
        if status.updated_at_unix_ms == 0 {
            return (String::new(), String::new(), 0, String::new());
        }

        let now_ms = Utc::now().timestamp_millis().max(0) as u64;
        let age_ms = now_ms.saturating_sub(status.updated_at_unix_ms);
        let editing_label = status
            .payload
            .get("activity_hint")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Teacher is preparing the next revision")
            .to_string();

        if age_ms <= 8_000 {
            let key = format!(
                "{}|editing|{}|{}",
                module_id.trim(),
                editing_label,
                status.updated_at_unix_ms / 2_000
            );
            return (
                "editing".to_string(),
                editing_label,
                status.updated_at_unix_ms,
                key,
            );
        }

        let idle_label = if self.networking_shared_chat_policy.host_authoritative {
            "Teacher is ready for the next revision"
        } else {
            "Teacher is connected in this lesson room"
        };
        let key = format!(
            "{}|idle|{}",
            module_id.trim(),
            status.updated_at_unix_ms / 15_000
        );
        (
            "idle".to_string(),
            idle_label.to_string(),
            status.updated_at_unix_ms,
            key,
        )
    }

    fn sync_shared_chat_host_presence(&mut self) {
        self.ensure_shared_chat_policy_defaults();
        if !(self.networking_shared_chat_policy.session_active
            && self.networking_shared_chat_policy.scope_kind == SharedChatScopeKind::Module
            && self.shared_chat_is_local_host())
        {
            self.networking_shared_chat_presence_key.clear();
            return;
        }

        let mut next_state = String::new();
        let mut next_label = String::new();
        let mut next_updated_at = 0_u64;
        let mut next_key = String::new();

        let scoped_id = self.networking_shared_chat_policy.scope_module_id.trim();
        if let Some(module) = self
            .modules
            .iter()
            .find(|module| module.manifest.id.trim() == scoped_id)
        {
            (next_state, next_label, next_updated_at, next_key) =
                self.derive_module_host_activity_presence(&module.manifest.id, &module.folder);
        }

        let changed = self.networking_shared_chat_policy.host_activity_state != next_state
            || self.networking_shared_chat_policy.host_activity_label != next_label
            || self
                .networking_shared_chat_policy
                .host_activity_updated_at_unix_ms
                != next_updated_at;
        let key_changed = self.networking_shared_chat_presence_key != next_key;
        if !changed && !key_changed {
            return;
        }

        self.networking_shared_chat_policy.host_activity_state = next_state;
        self.networking_shared_chat_policy.host_activity_label = next_label;
        self.networking_shared_chat_policy
            .host_activity_updated_at_unix_ms = next_updated_at;
        self.networking_shared_chat_presence_key = next_key;
        self.broadcast_shared_chat_policy_with_options("", false, false, false);
    }

    fn shared_chat_can_send_user_message(&self) -> Result<(), String> {
        if self.networking.snapshot().connected_peers.is_empty() {
            return Ok(());
        }
        if self.shared_chat_teacher_override_active() {
            return Ok(());
        }
        if self.networking_shared_chat_policy.turn_mode == SharedChatTurnMode::Open {
            return Ok(());
        }
        let local_id = self.networking.snapshot().device_id.clone();
        let holder = self
            .networking_shared_chat_policy
            .turn_holder_device_id
            .trim()
            .to_string();
        if holder.is_empty() || holder == local_id {
            Ok(())
        } else {
            Err(format!(
                "Talking stick is currently with {}.",
                self.shared_chat_turn_holder_label()
            ))
        }
    }

    fn shared_chat_can_send_mirrored_main_chat_message(&self) -> Result<(), String> {
        if !self.networking_shared_chat_mirror_main_chat {
            return Ok(());
        }
        self.shared_chat_can_send_user_message()
    }

    fn shared_chat_local_ai_allowed(&self) -> bool {
        if !self.networking_shared_chat_mirror_main_chat
            || self.networking.snapshot().connected_peers.is_empty()
        {
            return true;
        }
        match self.networking_shared_chat_policy.ai_mode {
            SharedChatAiMode::Off => false,
            SharedChatAiMode::LocalAllowed => true,
            SharedChatAiMode::HostOnly => self.shared_chat_is_local_host(),
        }
    }

    fn build_shared_chat_message(
        &self,
        speaker_kind: &str,
        speaker_label: &str,
        body: &str,
    ) -> SharedChatMessage {
        let snapshot = self.networking.snapshot().clone();
        let now_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
        SharedChatMessage {
            version: "1".to_string(),
            message_id: format!("room-{}-{}", snapshot.device_id, now_unix_ms),
            sent_at_unix_ms: now_unix_ms,
            source_app: "chatty-edu".to_string(),
            from_device_id: snapshot.device_id,
            from_device_name: snapshot.device_name,
            speaker_kind: speaker_kind.to_string(),
            speaker_label: speaker_label.to_string(),
            scope_kind: self.networking_shared_chat_policy.scope_kind,
            scope_module_id: self.networking_shared_chat_policy.scope_module_id.clone(),
            scope_module_name: self.networking_shared_chat_policy.scope_module_name.clone(),
            scope_multiplayer: self.networking_shared_chat_policy.scope_multiplayer,
            session_active: self.networking_shared_chat_policy.session_active,
            session_id: self.networking_shared_chat_policy.session_id.clone(),
            session_revision: self.networking_shared_chat_policy.session_revision,
            body: body.trim().to_string(),
        }
    }

    fn push_shared_chat_message_local(&mut self, message: SharedChatMessage) {
        if message.message_id.trim().is_empty()
            || self
                .networking_shared_chat_seen_messages
                .contains(&message.message_id)
        {
            return;
        }
        self.networking_shared_chat_seen_messages
            .insert(message.message_id.clone());
        self.networking_shared_chat_log.push(message);
        self.networking_shared_chat_log
            .sort_by_key(|entry| entry.sent_at_unix_ms);
        if self.networking_shared_chat_log.len() > 120 {
            let drop_count = self.networking_shared_chat_log.len() - 120;
            self.networking_shared_chat_log.drain(0..drop_count);
        }
    }

    fn add_shared_chat_notice(&mut self, body: impl Into<String>) {
        let body = body.into();
        if body.trim().is_empty() {
            return;
        }
        let message = self.build_shared_chat_message("system", "Room", &body);
        self.push_shared_chat_message_local(message);
    }

    fn broadcast_shared_chat_policy(&mut self, note: &str) {
        self.broadcast_shared_chat_policy_with_options(note, true, true, false);
    }

    fn broadcast_shared_chat_policy_with_options(
        &mut self,
        note: &str,
        bump_revision: bool,
        announce: bool,
        preserve_host_assignment: bool,
    ) {
        self.ensure_shared_chat_policy_defaults();
        let snapshot = self.networking.snapshot().clone();
        self.networking_shared_chat_policy.updated_at_unix_ms =
            Utc::now().timestamp_millis().max(0) as u64;
        self.networking_shared_chat_policy.source_app = "chatty-edu".to_string();
        if !preserve_host_assignment
            || self
                .networking_shared_chat_policy
                .host_device_id
                .trim()
                .is_empty()
        {
            self.networking_shared_chat_policy.host_device_id = snapshot.device_id;
            self.networking_shared_chat_policy.host_device_name = snapshot.device_name;
        }
        if self.networking_shared_chat_policy.session_active
            && self.networking_shared_chat_policy.scope_kind == SharedChatScopeKind::Module
            && bump_revision
        {
            self.networking_shared_chat_policy.session_revision = self
                .networking_shared_chat_policy
                .session_revision
                .saturating_add(1)
                .max(1);
            if self
                .networking_shared_chat_policy
                .session_label
                .trim()
                .is_empty()
            {
                self.networking_shared_chat_policy.session_label = format!(
                    "{} room session",
                    self.networking_shared_chat_policy.scope_module_name.trim()
                );
            }
        }
        if self.networking_shared_chat_policy.turn_mode == SharedChatTurnMode::Open {
            self.networking_shared_chat_policy
                .turn_holder_device_id
                .clear();
            self.networking_shared_chat_policy
                .turn_holder_device_name
                .clear();
            self.networking_turn_holder = None;
        } else {
            self.networking_turn_holder = Some(
                self.networking_shared_chat_policy
                    .turn_holder_device_id
                    .clone(),
            );
        }
        let summary = self.shared_chat_policy_summary();
        if announce {
            self.add_shared_chat_notice(if note.trim().is_empty() {
                format!("Shared room policy updated: {summary}.")
            } else {
                format!("Shared room policy updated: {summary}. {note}")
            });
        }

        match serde_json::to_string_pretty(&self.networking_shared_chat_policy) {
            Ok(text) => {
                let connection_ids = self.shared_chat_connected_connection_ids();
                for connection_id in &connection_ids {
                    self.networking.send_artifact(
                        connection_id,
                        "shared_chat_policy_json",
                        "Classroom room policy",
                        None,
                        &summary,
                        "shared_chat_policy.json",
                        &text,
                    );
                }
                if announce {
                    self.networking_status = Some(if connection_ids.is_empty() {
                        "Updated classroom room policy locally.".to_string()
                    } else {
                        format!(
                            "Shared room policy sent to {} connected peer(s).",
                            connection_ids.len()
                        )
                    });
                }
            }
            Err(err) => {
                self.networking_status =
                    Some(format!("Could not serialize shared room policy: {err}"));
            }
        }
        self.sync_recoverable_shared_chat_policy_snapshot();
    }

    fn apply_received_shared_chat_policy(
        &mut self,
        artifact: &ReceivedArtifact,
    ) -> anyhow::Result<()> {
        let mut policy: SharedChatPolicy = serde_json::from_str(&artifact.text)?;
        if policy.version.trim().is_empty() {
            policy.version = "1".to_string();
        }
        if policy.label.trim().is_empty() {
            policy.label = "Classroom room".to_string();
        }
        if policy.source_app.trim().is_empty() {
            policy.source_app = "chatty-edu".to_string();
        }
        if policy.scope_kind == SharedChatScopeKind::Module
            && policy.scope_module_id.trim().is_empty()
        {
            policy.scope_kind = SharedChatScopeKind::General;
            policy.scope_module_name.clear();
            policy.scope_multiplayer = false;
            policy.session_active = false;
            policy.session_id.clear();
            policy.session_revision = 0;
            policy.session_label.clear();
            policy.host_authoritative = false;
        }
        let should_apply = self.networking_shared_chat_policy.updated_at_unix_ms == 0
            || policy.updated_at_unix_ms >= self.networking_shared_chat_policy.updated_at_unix_ms;
        if !should_apply {
            self.networking_status = Some(format!(
                "Ignored older classroom room policy from {}.",
                artifact.from_device_name
            ));
            return Ok(());
        }
        let previous_policy = self.networking_shared_chat_policy.clone();
        let previously_local_host = previous_policy.host_device_id.trim().is_empty()
            || previous_policy.host_device_id.trim() == self.networking.snapshot().device_id.trim();
        self.networking_shared_chat_policy = policy;
        self.ensure_shared_chat_policy_defaults();
        let now_local_host = self.shared_chat_is_local_host();
        if now_local_host {
            self.networking_shared_chat_presence_next_sync_at = Some(Instant::now());
        }
        if previously_local_host && !now_local_host {
            self.networking_shared_chat_presence_key.clear();
        }
        let presence_only =
            previous_policy.equivalent_except_presence(&self.networking_shared_chat_policy);
        if !presence_only {
            self.add_shared_chat_notice(format!(
                "{} updated the classroom room: {}.",
                artifact.from_device_name,
                self.shared_chat_policy_summary()
            ));
            self.networking_status = Some(format!(
                "Shared room policy updated from {}.",
                artifact.from_device_name
            ));
        }
        self.sync_recoverable_shared_chat_policy_snapshot();
        Ok(())
    }

    fn broadcast_shared_chat_message(
        &mut self,
        speaker_kind: &str,
        speaker_label: &str,
        body: &str,
    ) {
        let connection_ids = self.shared_chat_connected_connection_ids();
        if connection_ids.is_empty() || body.trim().is_empty() {
            return;
        }
        let message = self.build_shared_chat_message(speaker_kind, speaker_label, body);
        let summary = Self::clip_chars(&body.replace('\n', " "), 96);
        self.push_shared_chat_message_local(message.clone());
        match serde_json::to_string_pretty(&message) {
            Ok(text) => {
                let file_name = format!(
                    "shared_chat_message_{}.json",
                    slugify_filename(&message.message_id, "shared_chat_message")
                );
                for connection_id in &connection_ids {
                    self.networking.send_artifact(
                        connection_id,
                        "shared_chat_message_json",
                        "Shared room message",
                        None,
                        &summary,
                        &file_name,
                        &text,
                    );
                }
                self.networking_status = Some(format!(
                    "Shared room message sent to {} connected peer(s).",
                    connection_ids.len()
                ));
            }
            Err(err) => {
                self.networking_status =
                    Some(format!("Could not serialize shared room message: {err}"));
            }
        }
    }

    fn apply_received_shared_chat_message(
        &mut self,
        artifact: &ReceivedArtifact,
    ) -> anyhow::Result<()> {
        let mut message: SharedChatMessage = serde_json::from_str(&artifact.text)?;
        if message.version.trim().is_empty() {
            message.version = "1".to_string();
        }
        if message.scope_kind == SharedChatScopeKind::Module
            && message.scope_module_id.trim().is_empty()
        {
            message.scope_kind = SharedChatScopeKind::General;
            message.scope_module_name.clear();
            message.scope_multiplayer = false;
            message.session_active = false;
            message.session_id.clear();
            message.session_revision = 0;
        }
        if message.message_id.trim().is_empty() {
            let now_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
            message.message_id = format!("room-{}-{}", artifact.from_device_id, now_unix_ms);
        }
        if message.from_device_name.trim().is_empty() {
            message.from_device_name = artifact.from_device_name.clone();
        }
        if message.from_device_id.trim().is_empty() {
            message.from_device_id = artifact.from_device_id.clone();
        }
        self.push_shared_chat_message_local(message);
        self.networking_status = Some(format!(
            "Shared room message received from {}.",
            artifact.from_device_name
        ));
        Ok(())
    }

    fn build_module_shared_room_state(
        &self,
        module: &LoadedModule,
    ) -> Option<ModuleBridgeSharedRoomState> {
        let caps = module.manifest.network_capabilities.as_ref()?;
        let room_aware = caps.has(ModuleNetworkFeature::RoomAware);
        let multiplayer = caps.has(ModuleNetworkFeature::Multiplayer);
        if !room_aware && !multiplayer {
            return None;
        }
        let scope_matches = self.shared_chat_scope_matches_module(&module.manifest.id);
        let active_for_module = self.networking_shared_chat_policy.scope_kind
            == SharedChatScopeKind::General
            || scope_matches;
        let snapshot = self.networking.snapshot().clone();
        let local_id = snapshot.device_id.clone();
        let local_name = snapshot.device_name.clone();
        let local_has_turn = self.networking_shared_chat_policy.turn_mode
            == SharedChatTurnMode::Open
            || self
                .networking_shared_chat_policy
                .turn_holder_device_id
                .trim()
                .is_empty()
            || self
                .networking_shared_chat_policy
                .turn_holder_device_id
                .trim()
                == local_id.trim();
        let mut participants = vec![ModuleBridgeSharedRoomParticipant {
            device_id: local_id.clone(),
            device_name: local_name.clone(),
            is_local: true,
            connected: true,
        }];
        participants.extend(snapshot.connected_peers.iter().map(|peer| {
            ModuleBridgeSharedRoomParticipant {
                device_id: peer.device_id.clone(),
                device_name: self.network_display_name(&peer.device_id, &peer.device_name),
                is_local: false,
                connected: true,
            }
        }));
        let participant_count = participants.len();
        Some(ModuleBridgeSharedRoomState {
            version: "1".to_string(),
            source_app: "chatty-edu".to_string(),
            label: self.networking_shared_chat_policy.label.clone(),
            scope_kind: match self.networking_shared_chat_policy.scope_kind {
                SharedChatScopeKind::General => "general".to_string(),
                SharedChatScopeKind::Module => "module".to_string(),
            },
            scope_module_id: self.networking_shared_chat_policy.scope_module_id.clone(),
            scope_module_name: self.networking_shared_chat_policy.scope_module_name.clone(),
            scope_multiplayer: self.networking_shared_chat_policy.scope_multiplayer,
            active_for_module,
            session_active: self.networking_shared_chat_policy.session_active,
            session_id: self.networking_shared_chat_policy.session_id.clone(),
            session_revision: self.networking_shared_chat_policy.session_revision,
            session_label: self.networking_shared_chat_policy.session_label.clone(),
            host_authoritative: self.networking_shared_chat_policy.host_authoritative,
            turn_mode: self
                .networking_shared_chat_policy
                .turn_mode
                .label()
                .to_string(),
            ai_mode: self
                .networking_shared_chat_policy
                .ai_mode
                .label()
                .to_string(),
            teacher_override: self.networking_shared_chat_policy.teacher_override,
            host_device_id: self.networking_shared_chat_policy.host_device_id.clone(),
            host_device_name: self.networking_shared_chat_policy.host_device_name.clone(),
            turn_holder_device_id: self
                .networking_shared_chat_policy
                .turn_holder_device_id
                .clone(),
            turn_holder_device_name: self
                .networking_shared_chat_policy
                .turn_holder_device_name
                .clone(),
            connected_peer_count: self.shared_chat_connected_connection_ids().len(),
            participant_count,
            local_device_id: local_id,
            local_device_name: local_name,
            local_is_host: self.shared_chat_is_local_host(),
            local_has_turn,
            host_activity_state: self
                .networking_shared_chat_policy
                .host_activity_state
                .clone(),
            host_activity_label: self
                .networking_shared_chat_policy
                .host_activity_label
                .clone(),
            host_activity_updated_at_unix_ms: self
                .networking_shared_chat_policy
                .host_activity_updated_at_unix_ms,
            participants,
            summary: self.shared_chat_policy_summary(),
            updated_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
        })
    }

    fn build_module_shared_room_events(
        &self,
        module: &LoadedModule,
    ) -> Option<ModuleBridgeSharedRoomEvents> {
        let room_state = self.build_module_shared_room_state(module)?;
        if !room_state.active_for_module {
            return None;
        }

        let module_id = module.manifest.id.trim();
        let session_id = room_state.session_id.trim();
        let mut events = self
            .networking
            .snapshot()
            .received_session_events
            .iter()
            .filter(|event| {
                let scope = event.scope_module_id.trim();
                let scope_matches = scope.is_empty() || scope == module_id;
                let session_matches = session_id.is_empty()
                    || event.session_id.trim().is_empty()
                    || event.session_id.trim() == session_id;
                scope_matches && session_matches
            })
            .map(|event| ModuleBridgeRoomEvent {
                event_id: event.event_id.clone(),
                source_app: "chatty-edu-lan".to_string(),
                scope_module_id: event.scope_module_id.clone(),
                session_id: event.session_id.clone(),
                event_type: event.event_type.clone(),
                label: event.label.clone(),
                content_type: event.content_type.clone(),
                payload_text: event.payload_text.clone(),
                from_device_id: event.from_device_id.clone(),
                from_device_name: event.from_device_name.clone(),
                local_echo: false,
                sent_at_unix_ms: event.received_at_unix_ms,
                received_at_unix_ms: event.received_at_unix_ms,
            })
            .collect::<Vec<_>>();

        if events.is_empty() {
            return None;
        }

        events.sort_by_key(|event| event.received_at_unix_ms);
        Some(ModuleBridgeSharedRoomEvents {
            version: "1".to_string(),
            source_app: "chatty-edu".to_string(),
            scope_module_id: module.manifest.id.clone(),
            session_id: room_state.session_id.clone(),
            session_revision: room_state.session_revision,
            updated_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
            events,
        })
    }

    fn sync_module_shared_room_bridge_state(&mut self) {
        for module in &self.modules {
            let state = self.build_module_shared_room_state(module);
            let Some(state) = state else {
                let _ = clear_bridge_shared_room_state(&module.folder);
                self.module_room_bridge_last_fingerprint
                    .remove(&module.manifest.id);
                continue;
            };
            let fingerprint = state.fingerprint();
            let needs_write = self
                .module_room_bridge_last_fingerprint
                .get(&module.manifest.id)
                .map(|existing| existing != &fingerprint)
                .unwrap_or(true);
            if needs_write && write_bridge_shared_room_state(&module.folder, &state).is_ok() {
                self.module_room_bridge_last_fingerprint
                    .insert(module.manifest.id.clone(), fingerprint);
            }
        }
    }

    fn sync_module_shared_room_events_bridge(&mut self) {
        for module in &self.modules {
            let events = self.build_module_shared_room_events(module);
            let Some(events) = events else {
                let _ = clear_bridge_shared_room_events(&module.folder);
                self.module_room_events_bridge_last_fingerprint
                    .remove(&module.manifest.id);
                continue;
            };
            let fingerprint = serde_json::to_string(&events).unwrap_or_default();
            let needs_write = self
                .module_room_events_bridge_last_fingerprint
                .get(&module.manifest.id)
                .map(|existing| existing != &fingerprint)
                .unwrap_or(true);
            if needs_write && write_bridge_shared_room_events(&module.folder, &events).is_ok() {
                self.module_room_events_bridge_last_fingerprint
                    .insert(module.manifest.id.clone(), fingerprint);
            }
        }
    }

    fn process_module_outgoing_room_events(&mut self) {
        let snapshot = self.networking.snapshot().clone();
        let room_connection_ids = self.shared_chat_connected_connection_ids();
        for module in &self.modules {
            let Some(caps) = module.manifest.network_capabilities.as_ref() else {
                let _ = clear_bridge_outgoing_room_events(&module.folder);
                continue;
            };
            if !caps.has(ModuleNetworkFeature::RoomAware)
                && !caps.has(ModuleNetworkFeature::Multiplayer)
            {
                let _ = clear_bridge_outgoing_room_events(&module.folder);
                continue;
            }
            let outgoing_events = match read_bridge_outgoing_room_events(&module.folder) {
                Ok(events) => events,
                Err(err) => {
                    self.networking_status = Some(format!(
                        "Could not read outgoing room events for {}: {err}",
                        module.manifest.title
                    ));
                    continue;
                }
            };
            if outgoing_events.is_empty() {
                continue;
            }
            let Some(room_state) = self.build_module_shared_room_state(module) else {
                continue;
            };
            if !room_state.active_for_module || room_connection_ids.is_empty() {
                continue;
            }

            let session_id = if room_state.session_active {
                room_state.session_id.clone()
            } else {
                String::new()
            };
            let local_name = snapshot.device_name.clone();
            let local_id = snapshot.device_id.clone();
            for mut event in outgoing_events.iter().cloned() {
                event.normalize();
                if event.event_id.trim().is_empty() {
                    event.event_id = format!(
                        "edu-room-{}-{}",
                        module.manifest.id,
                        Utc::now().timestamp_millis().max(0) as u64
                    );
                }
                let label = if event.label.trim().is_empty() {
                    format!("{} event", module.manifest.title.trim())
                } else {
                    event.label.clone()
                };
                for connection_id in &room_connection_ids {
                    self.networking.send_session_event(
                        connection_id,
                        &module.manifest.id,
                        &session_id,
                        &event.event_type,
                        &label,
                        &event.content_type,
                        &event.payload_text,
                    );
                }
                self.networking_status = Some(format!(
                    "Relayed {} room event(s) from {} to {} connected device(s).",
                    outgoing_events.len(),
                    if local_name.trim().is_empty() {
                        local_id.trim()
                    } else {
                        local_name.trim()
                    },
                    room_connection_ids.len()
                ));
            }
            let _ = clear_bridge_outgoing_room_events(&module.folder);
        }
    }

    fn network_inbox_dir(&self) -> PathBuf {
        self.base_path.join("network_inbox")
    }

    fn network_recovery_dir(&self) -> PathBuf {
        self.base_path.join("network_recovery")
    }

    fn recoverable_module_session_path(&self) -> PathBuf {
        self.network_recovery_dir()
            .join("recoverable_module_session.json")
    }

    fn recoverable_module_session_payload_dir(&self) -> PathBuf {
        self.network_recovery_dir().join("module_session_payloads")
    }

    fn received_homework_inbox_dir(&self) -> PathBuf {
        self.network_inbox_dir().join("homework_packs")
    }

    fn received_revision_inbox_dir(&self) -> PathBuf {
        self.network_inbox_dir().join("revision_packs")
    }

    fn received_lukewarm_inbox_dir(&self) -> PathBuf {
        self.network_inbox_dir().join("lukewarm_context")
    }

    fn applied_lukewarm_dir(&self) -> PathBuf {
        self.network_inbox_dir().join("applied_lukewarm_context")
    }

    fn received_transfer_inbox_dir(&self) -> PathBuf {
        self.network_inbox_dir().join("file_transfers")
    }

    fn received_transfer_payload_dir(&self) -> PathBuf {
        self.received_transfer_inbox_dir().join("payloads")
    }

    fn applied_transfer_dir(&self) -> PathBuf {
        self.network_inbox_dir()
            .join("imports")
            .join("network_transfers")
    }

    fn received_bundle_inbox_dir(&self) -> PathBuf {
        self.network_inbox_dir().join("workflow_bundles")
    }

    fn sync_selected_received_homework_pack(&mut self) {
        let still_exists = self
            .selected_received_homework_pack
            .as_ref()
            .is_some_and(|path| {
                self.received_homework_inbox
                    .iter()
                    .any(|item| &item.path == path)
            });
        if !still_exists {
            self.selected_received_homework_pack = self
                .received_homework_inbox
                .first()
                .map(|item| item.path.clone());
        }
    }

    fn refresh_received_homework_inbox(&mut self) {
        self.received_homework_inbox =
            load_received_homework_pack_inbox(&self.base_path).unwrap_or_default();
        self.sync_selected_received_homework_pack();
    }

    fn sync_selected_received_revision_pack(&mut self) {
        let still_exists = self
            .selected_received_revision_pack
            .as_ref()
            .is_some_and(|path| {
                self.received_revision_inbox
                    .iter()
                    .any(|item| &item.path == path)
            });
        if !still_exists {
            self.selected_received_revision_pack = self
                .received_revision_inbox
                .first()
                .map(|item| item.path.clone());
        }
    }

    fn refresh_received_revision_inbox(&mut self) {
        self.received_revision_inbox =
            load_received_revision_pack_inbox(&self.base_path).unwrap_or_default();
        self.sync_selected_received_revision_pack();
    }

    fn sync_selected_received_lukewarm(&mut self) {
        let still_exists = self
            .selected_received_lukewarm
            .as_ref()
            .is_some_and(|path| {
                self.received_lukewarm_inbox
                    .iter()
                    .any(|item| &item.path == path)
            });
        if !still_exists {
            self.selected_received_lukewarm = self
                .received_lukewarm_inbox
                .first()
                .map(|item| item.path.clone());
        }
    }

    fn refresh_received_lukewarm_inbox(&mut self) {
        self.received_lukewarm_inbox =
            load_received_lukewarm_inbox(&self.base_path).unwrap_or_default();
        self.sync_selected_received_lukewarm();
    }

    fn sync_selected_received_transfer(&mut self) {
        let still_exists = self
            .selected_received_transfer
            .as_ref()
            .is_some_and(|path| {
                self.received_transfer_inbox
                    .iter()
                    .any(|item| &item.path == path)
            });
        if !still_exists {
            self.selected_received_transfer = self
                .received_transfer_inbox
                .first()
                .map(|item| item.path.clone());
        }
    }

    fn refresh_received_transfer_inbox(&mut self) {
        self.received_transfer_inbox =
            load_received_generic_transfer_inbox(&self.base_path).unwrap_or_default();
        self.sync_selected_received_transfer();
    }

    fn sync_selected_received_bundle(&mut self) {
        let still_exists = self.selected_received_bundle.as_ref().is_some_and(|path| {
            self.received_bundle_inbox
                .iter()
                .any(|item| &item.path == path)
        });
        if !still_exists {
            self.selected_received_bundle = self
                .received_bundle_inbox
                .first()
                .map(|item| item.path.clone());
        }
    }

    fn refresh_received_bundle_inbox(&mut self) {
        self.received_bundle_inbox =
            load_received_workflow_bundle_inbox(&self.base_path).unwrap_or_default();
        self.sync_selected_received_bundle();
    }

    fn build_current_lukewarm_share(&self) -> SharedLukewarmContext {
        let items = self.memory_jogger_items();
        let context_text = if items.is_empty() {
            String::new()
        } else {
            format!(
                "### Memory Jogger\n- {}",
                items
                    .into_iter()
                    .take(12)
                    .map(|item| Self::clip_chars(item.trim(), 220))
                    .collect::<Vec<_>>()
                    .join("\n- ")
            )
        };
        let summary = if context_text.trim().is_empty() {
            "No current EDU memory jogger summary is available yet.".to_string()
        } else {
            "Shareable EDU memory jogger summary.".to_string()
        };
        let snapshot = self.networking.snapshot().clone();
        SharedLukewarmContext {
            version: "1.0".to_string(),
            label: "Chatty-EDU recent context".to_string(),
            summary,
            created_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
            source_app: "Chatty-EDU".to_string(),
            source_device_id: snapshot.device_id,
            source_device_name: snapshot.device_name,
            context_text,
        }
    }

    fn build_applied_lukewarm_context_block(&self) -> String {
        if !self.settings.network_allow_shared_lukewarm_context {
            return String::new();
        }
        let items = load_applied_lukewarm_contexts(&self.base_path).unwrap_or_default();
        if items.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for item in items.into_iter().take(6) {
            let label = if item.record.from_device_name.trim().is_empty() {
                item.record.from_device_id.as_str()
            } else {
                item.record.from_device_name.as_str()
            };
            let text = item.record.context.context_text.trim();
            if text.is_empty() {
                continue;
            }
            out.push_str("Shared from ");
            out.push_str(label);
            out.push_str(":\n");
            out.push_str(&Self::clip_chars(text, 1_200));
            out.push_str("\n\n");
            if out.len() >= 4_000 {
                break;
            }
        }
        out.trim().to_string()
    }

    fn portable_model_hint(&self, path_text: &str) -> Option<String> {
        let trimmed = path_text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let path = Path::new(trimmed);
        let modules_dir = self.base_path.join("modules");
        if let Ok(rel) = path.strip_prefix(&modules_dir) {
            return Some(format!(
                "modules/{}",
                rel.to_string_lossy().replace('\\', "/")
            ));
        }
        let models_dir = self.base_path.join("models");
        if let Ok(rel) = path.strip_prefix(&models_dir) {
            return Some(rel.to_string_lossy().replace('\\', "/"));
        }
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
    }

    fn resolve_portable_model_hint(&self, hint: Option<&str>) -> Option<PathBuf> {
        let hint = hint?.trim();
        if hint.is_empty() {
            return None;
        }

        if let Some(rest) = hint.strip_prefix("modules/") {
            let path = self.base_path.join("modules").join(rest.replace('/', "\\"));
            if path.is_file() {
                return Some(path);
            }
        }

        let models_dir = self.base_path.join("models");
        let direct = models_dir.join(hint.replace('/', "\\"));
        if direct.is_file() {
            return Some(direct);
        }
        let by_name = models_dir.join(hint);
        if by_name.is_file() {
            return Some(by_name);
        }

        let file_name = Path::new(hint)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())?;
        discover_local_models(&self.base_path)
            .into_iter()
            .find(|model| {
                model
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().eq_ignore_ascii_case(&file_name))
                    .unwrap_or(false)
            })
            .map(|model| model.path)
    }

    fn build_current_workflow_bundle(&self) -> WorkflowBundle {
        let label = if self.networking_bundle_label.trim().is_empty() {
            "Classroom setup".to_string()
        } else {
            self.networking_bundle_label.trim().to_string()
        };
        let summary = if self.networking_bundle_summary.trim().is_empty() {
            format!(
                "Teacher mode: {} | Janet: {} | Games in class: {}",
                self.settings.teacher_mode,
                if self.settings.janet.enabled {
                    "on"
                } else {
                    "off"
                },
                if self.settings.game.games_in_class_allowed {
                    "allowed"
                } else {
                    "off"
                }
            )
        } else {
            self.networking_bundle_summary.trim().to_string()
        };
        WorkflowBundle {
            version: "1.0".to_string(),
            label,
            summary,
            created_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
            teacher_mode: self.settings.teacher_mode.clone(),
            default_year_level: self.settings.default_year_level.clone(),
            homework_hints_only: self.settings.homework_hints_only,
            janet: self.settings.janet.clone(),
            model_hint: self.portable_model_hint(&self.settings.model.path),
            model_name: self.settings.model.name.clone(),
            model_max_tokens: self.settings.model.max_tokens,
            bookkeeper_model_hint: self.portable_model_hint(&self.settings.bookkeeper_model_path),
            bookkeeper_model_name: self.settings.bookkeeper_model_name.clone(),
            voice: self.settings.voice.clone(),
            game: self.settings.game.clone(),
        }
    }

    fn connected_connection_id_for_device(&self, device_id: &str) -> Option<String> {
        let wanted = device_id.trim();
        if wanted.is_empty() {
            return None;
        }
        self.networking
            .snapshot()
            .connected_peers
            .iter()
            .find(|peer| peer.device_id.trim() == wanted)
            .map(|peer| peer.connection_id.clone())
    }

    fn prepare_outgoing_module_shared_state(
        &mut self,
        module_id: &str,
        shared_state: &ModuleBridgeSharedState,
    ) -> ModuleBridgeSharedState {
        let fingerprint = shared_state.content_fingerprint();
        let snapshot = self.networking.snapshot().clone();
        let now = Utc::now().timestamp_millis().max(0) as u64;
        let tracker = self
            .module_session_trackers
            .entry(module_id.to_string())
            .or_insert_with(|| ModuleSessionTracker {
                session_id: format!(
                    "session-{}-{}-{}",
                    slugify_filename(module_id, "module"),
                    slugify_filename(&snapshot.device_id, "device"),
                    now
                ),
                ..ModuleSessionTracker::default()
            });

        if tracker.last_revision == 0 {
            tracker.last_revision = 1;
        }
        if tracker.last_fingerprint.trim().is_empty() {
            tracker.last_fingerprint = fingerprint.clone();
        } else if tracker.last_fingerprint != fingerprint {
            tracker.last_revision += 1;
            tracker.last_fingerprint = fingerprint.clone();
        }
        tracker.last_shared_at_unix_ms = now;

        let mut prepared = shared_state.clone();
        prepared.module_id = module_id.trim().to_string();
        prepared.session_id = tracker.session_id.clone();
        prepared.session_revision = tracker.last_revision;
        prepared.authoritative_device_id = snapshot.device_id;
        prepared.authoritative_device_name = snapshot.device_name;
        prepared.host_authoritative = true;
        prepared.updated_at_unix_ms = now;
        prepared
    }

    fn reset_module_shared_session(&mut self, module_id: &str) {
        self.module_session_trackers.remove(module_id);
        self.module_session_receipts
            .retain(|receipt| receipt.module_id.trim() != module_id.trim());
    }

    fn module_session_receipts_for(&self, module_id: &str) -> Vec<ModuleSessionAckRecord> {
        let mut items = self
            .module_session_receipts
            .iter()
            .filter(|receipt| receipt.module_id.trim() == module_id.trim())
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .acknowledged_at_unix_ms
                .cmp(&left.acknowledged_at_unix_ms)
        });
        items
    }

    fn send_module_session_ack(
        &mut self,
        module_id: &str,
        target_device_id: &str,
        target_device_name: &str,
        state: &ModuleBridgeSharedState,
        applied: bool,
        stale: bool,
        message: &str,
    ) {
        let Some(connection_id) = self.connected_connection_id_for_device(target_device_id) else {
            return;
        };
        let ack = ModuleSessionAckRecord {
            module_id: module_id.trim().to_string(),
            session_id: state.session_id.clone(),
            session_revision: state.session_revision,
            from_device_id: self.networking.snapshot().device_id.clone(),
            from_device_name: self.networking.snapshot().device_name.clone(),
            applied,
            stale,
            message: message.trim().to_string(),
            acknowledged_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
        };
        if let Ok(text) = serde_json::to_string_pretty(&ack) {
            self.networking.send_artifact(
                &connection_id,
                "module_shared_state_ack_json",
                &format!("{} session ack", module_id),
                Some(module_id),
                message,
                &format!("{}_session_ack.json", slugify_filename(module_id, "module")),
                &text,
            );
        } else {
            self.networking_status = Some(format!(
                "Could not serialize a session receipt back to {}.",
                target_device_name
            ));
        }
    }

    fn stale_module_state_message(
        &self,
        module_dir: &Path,
        state: &ModuleBridgeSharedState,
    ) -> Option<String> {
        if state.session_id.trim().is_empty() || state.session_revision == 0 {
            return None;
        }
        let existing = read_bridge_incoming_shared_state(module_dir)
            .ok()
            .flatten()?;
        if existing.session_id.trim() == state.session_id.trim()
            && existing.session_revision >= state.session_revision
        {
            Some(format!(
                "Session revision {} is older than or equal to the already applied revision {}.",
                state.session_revision, existing.session_revision
            ))
        } else {
            None
        }
    }

    fn store_received_module_session_ack(
        &mut self,
        artifact: &ReceivedArtifact,
    ) -> io::Result<ModuleSessionAckRecord> {
        let mut ack: ModuleSessionAckRecord =
            serde_json::from_str(&artifact.text).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("module session ack parse error: {err}"),
                )
            })?;
        ack.module_id = ack.module_id.trim().to_string();
        ack.session_id = ack.session_id.trim().to_string();
        ack.from_device_id = if ack.from_device_id.trim().is_empty() {
            artifact.from_device_id.clone()
        } else {
            ack.from_device_id.trim().to_string()
        };
        ack.from_device_name = if ack.from_device_name.trim().is_empty() {
            artifact.from_device_name.clone()
        } else {
            ack.from_device_name.trim().to_string()
        };
        ack.message = ack.message.trim().to_string();
        if ack.acknowledged_at_unix_ms == 0 {
            ack.acknowledged_at_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
        }
        self.module_session_receipts.retain(|existing| {
            !(existing.module_id == ack.module_id
                && existing.session_id == ack.session_id
                && existing.session_revision == ack.session_revision
                && existing.from_device_id == ack.from_device_id)
        });
        self.module_session_receipts.insert(0, ack.clone());
        self.module_session_receipts.truncate(64);
        Ok(ack)
    }

    fn store_received_homework_pack(&mut self, artifact: &ReceivedArtifact) -> io::Result<PathBuf> {
        let pack: HomeworkPack = serde_json::from_str(&artifact.text).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("pack parse error: {err}"),
            )
        })?;

        let dir = self.received_homework_inbox_dir();
        fs::create_dir_all(&dir)?;
        let safe_sender = slugify_filename(&artifact.from_device_name, "peer");
        let safe_label = slugify_filename(
            if artifact.label.trim().is_empty() {
                "received_pack"
            } else {
                artifact.label.trim()
            },
            "received_pack",
        );
        let file_name = format!(
            "received_homework_pack_{}_{}_{}.json",
            safe_sender,
            safe_label,
            Utc::now().format("%Y%m%d_%H%M%S")
        );
        let path = dir.join(file_name);

        let record = ReceivedHomeworkPackRecord {
            artifact_id: artifact.artifact_id.clone(),
            from_device_id: artifact.from_device_id.clone(),
            from_device_name: artifact.from_device_name.clone(),
            label: artifact.label.clone(),
            summary: artifact.summary.clone(),
            file_name: artifact.file_name.clone(),
            received_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
            pack,
        };

        let bytes = serde_json::to_vec_pretty(&record).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("inbox serialize error: {err}"),
            )
        })?;
        fs::write(&path, bytes)?;
        self.refresh_received_homework_inbox();
        self.selected_received_homework_pack = Some(path.clone());
        self.networking_status = Some(format!(
            "Received pack from {} and saved it to the inbox.",
            artifact.from_device_name,
        ));
        Ok(path)
    }

    fn accept_received_homework_pack(&mut self, path: &Path) -> io::Result<PathBuf> {
        let mut record = read_received_homework_pack_record(path)?;
        record.pack.created_at = Utc::now().to_rfc3339();

        let dest_dir = self.base_path.join("homework").join("assigned");
        fs::create_dir_all(&dest_dir)?;
        let file_name = format!(
            "homework_pack_{}_network_{}_{}.json",
            slugify_filename(&record.pack.class_id, "class"),
            slugify_filename(&record.from_device_name, "peer"),
            Utc::now().format("%Y%m%d_%H%M%S")
        );
        let dest = dest_dir.join(file_name);
        let bytes = serde_json::to_vec_pretty(&record.pack).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("pack serialize error: {err}"),
            )
        })?;
        fs::write(&dest, bytes)?;

        apply_pack_policy(&mut self.settings, &record.pack);
        let _ = save_settings(&self.settings, &self.base_path);
        self.current_pack = Some(record.pack);
        self.refresh_homework_question_index();
        self.resync_homework();

        fs::remove_file(path)?;
        self.refresh_received_homework_inbox();
        self.networking_status = Some(format!(
            "Applied the received pack from {}.",
            record.from_device_name
        ));
        Ok(dest)
    }

    fn dismiss_received_homework_pack(&mut self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)?;
        self.refresh_received_homework_inbox();
        self.networking_status = Some("Dismissed a received homework pack.".to_string());
        Ok(())
    }

    fn render_received_homework_pack_inbox(&mut self, ui: &mut egui::Ui, heading: &str) {
        self.sync_selected_received_homework_pack();
        ui.heading(heading);
        ui.label(
            "Network-delivered homework packs land here first so you can preview them before making one live on this device.",
        );
        ui.horizontal(|ui| {
            if ui.button("Refresh inbox").clicked() {
                self.refresh_received_homework_inbox();
            }
            ui.small(format!(
                "{} pack(s) waiting",
                self.received_homework_inbox.len()
            ));
        });

        if self.received_homework_inbox.is_empty() {
            ui.small("No received packs are waiting right now.");
            return;
        }

        self.sync_selected_received_homework_pack();
        let selected_path = self.selected_received_homework_pack.clone().or_else(|| {
            self.received_homework_inbox
                .first()
                .map(|item| item.path.clone())
        });
        let selected_item = selected_path.as_ref().and_then(|path| {
            self.received_homework_inbox
                .iter()
                .find(|item| &item.path == path)
                .cloned()
        });

        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                ScrollArea::vertical()
                    .id_source((heading, "received_homework_inbox_list"))
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for item in &self.received_homework_inbox {
                            let selected = self
                                .selected_received_homework_pack
                                .as_ref()
                                .is_some_and(|path| path == &item.path);
                            let title = if item.record.label.trim().is_empty() {
                                item.record.pack.class_id.clone()
                            } else {
                                item.record.label.clone()
                            };
                            let subtitle = format!(
                                "{} | {} assignment(s)",
                                item.record.from_device_name,
                                item.record.pack.assignments.len()
                            );
                            ui.group(|ui| {
                                if ui
                                    .selectable_label(selected, title)
                                    .on_hover_text(item.path.display().to_string())
                                    .clicked()
                                {
                                    self.selected_received_homework_pack = Some(item.path.clone());
                                }
                                ui.small(subtitle);
                                if !item.record.summary.trim().is_empty() {
                                    ui.small(Self::clip_chars(item.record.summary.trim(), 120));
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
            });

            cols[1].vertical(|ui| {
                if let Some(item) = selected_item {
                    let record = item.record.clone();
                    let path = item.path.clone();
                    let title = if record.label.trim().is_empty() {
                        format!("Pack for {}", record.pack.class_id)
                    } else {
                        record.label.clone()
                    };
                    ui.label(RichText::new(title).strong());
                    ui.small(format!(
                        "From {} ({})",
                        record.from_device_name, record.from_device_id
                    ));
                    ui.small(format!(
                        "Class {} | {} assignment(s)",
                        record.pack.class_id,
                        record.pack.assignments.len()
                    ));
                    ui.small(format!("Saved as {}", path.display()));
                    if !record.summary.trim().is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Teacher note").strong());
                        ui.label(record.summary.trim());
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Apply pack now").clicked() {
                            if let Err(err) = self.accept_received_homework_pack(&path) {
                                self.networking_status =
                                    Some(format!("Could not apply the received pack: {}", err));
                            }
                        }
                        if ui.button("Dismiss").clicked() {
                            if let Err(err) = self.dismiss_received_homework_pack(&path) {
                                self.networking_status =
                                    Some(format!("Could not dismiss the received pack: {}", err));
                            }
                        }
                        if ui.button("Open file").clicked() {
                            open_path_in_explorer(&path);
                        }
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("Assignments").strong());
                    ScrollArea::vertical()
                        .id_source((heading, "received_homework_inbox_preview"))
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for assignment in &record.pack.assignments {
                                ui.group(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} - {}",
                                            assignment.id, assignment.title
                                        ))
                                        .strong(),
                                    );
                                    ui.small(format!(
                                        "{} | Year {}{}",
                                        assignment.subject,
                                        assignment.year_level,
                                        assignment
                                            .due_at
                                            .as_ref()
                                            .map(|due| format!(" | Due {}", due))
                                            .unwrap_or_default()
                                    ));
                                    if !assignment.instructions_md.trim().is_empty() {
                                        ui.small(Self::clip_chars(
                                            assignment.instructions_md.trim(),
                                            220,
                                        ));
                                    }
                                });
                                ui.add_space(4.0);
                            }
                        });
                } else {
                    ui.small("Select a received pack to preview it.");
                }
            });
        });
    }

    fn store_received_revision_pack(&mut self, artifact: &ReceivedArtifact) -> io::Result<PathBuf> {
        let dir = self.received_revision_inbox_dir();
        fs::create_dir_all(&dir)?;
        let safe_sender = slugify_filename(&artifact.from_device_name, "peer");
        let safe_label = slugify_filename(
            if artifact.label.trim().is_empty() {
                "received_revision_pack"
            } else {
                artifact.label.trim()
            },
            "received_revision_pack",
        );
        let file_name = format!(
            "received_revision_pack_{}_{}_{}.json",
            safe_sender,
            safe_label,
            Utc::now().format("%Y%m%d_%H%M%S")
        );
        let path = dir.join(file_name);

        let record = ReceivedRevisionPackRecord {
            artifact_id: artifact.artifact_id.clone(),
            from_device_id: artifact.from_device_id.clone(),
            from_device_name: artifact.from_device_name.clone(),
            label: artifact.label.clone(),
            summary: artifact.summary.clone(),
            file_name: artifact.file_name.clone(),
            received_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
            markdown: artifact.text.clone(),
        };

        let bytes = serde_json::to_vec_pretty(&record).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("revision inbox serialize error: {err}"),
            )
        })?;
        fs::write(&path, bytes)?;
        self.refresh_received_revision_inbox();
        self.selected_received_revision_pack = Some(path.clone());
        self.networking_status = Some(format!(
            "Received revision pack from {} and saved it to the inbox.",
            artifact.from_device_name,
        ));
        Ok(path)
    }

    fn accept_received_revision_pack(&mut self, path: &Path) -> io::Result<PathBuf> {
        let record = read_received_revision_pack_record(path)?;
        let dest_dir = revision_dir(&self.base_path).join("received");
        fs::create_dir_all(&dest_dir)?;
        let stem_hint = if record.file_name.trim().is_empty() {
            record.label.as_str()
        } else {
            record.file_name.as_str()
        };
        let dest = dest_dir.join(format!(
            "revision_pack_network_{}_{}_{}.md",
            slugify_filename(&record.from_device_name, "peer"),
            slugify_filename(stem_hint, "revision_pack"),
            Utc::now().format("%Y%m%d_%H%M%S")
        ));
        let markdown = record.markdown.trim();
        let text = if markdown.is_empty() {
            "# Revision Pack\n\n_This received pack was empty._\n".to_string()
        } else {
            format!("{}\n", markdown)
        };
        fs::write(&dest, text)?;
        self.resync_revision();
        fs::remove_file(path)?;
        self.refresh_received_revision_inbox();
        self.teacher_revision_status = Some(format!(
            "Saved the received revision pack from {} to {}",
            record.from_device_name,
            dest.display()
        ));
        self.networking_status = Some(format!(
            "Applied the received revision pack from {}.",
            record.from_device_name
        ));
        Ok(dest)
    }

    fn dismiss_received_revision_pack(&mut self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)?;
        self.refresh_received_revision_inbox();
        self.networking_status = Some("Dismissed a received revision pack.".to_string());
        Ok(())
    }

    fn store_received_lukewarm_context(
        &mut self,
        artifact: &ReceivedArtifact,
    ) -> io::Result<PathBuf> {
        let mut context: SharedLukewarmContext =
            serde_json::from_str(&artifact.text).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("lukewarm context parse error: {err}"),
                )
            })?;
        context.label = context.label.trim().to_string();
        context.summary = context.summary.trim().to_string();
        context.context_text = context.context_text.trim().to_string();
        if context.source_device_id.trim().is_empty() {
            context.source_device_id = artifact.from_device_id.clone();
        }
        if context.source_device_name.trim().is_empty() {
            context.source_device_name = artifact.from_device_name.clone();
        }
        if context.created_at_unix_ms == 0 {
            context.created_at_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
        }

        let dir = self.received_lukewarm_inbox_dir();
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!(
            "lukewarm_context_{}_{}_{}.json",
            slugify_filename(&artifact.from_device_name, "peer"),
            slugify_filename(
                if context.label.trim().is_empty() {
                    "lukewarm_context"
                } else {
                    context.label.trim()
                },
                "lukewarm_context"
            ),
            Utc::now().format("%Y%m%d_%H%M%S")
        ));
        let record = ReceivedLukewarmContextRecord {
            artifact_id: artifact.artifact_id.clone(),
            from_device_id: artifact.from_device_id.clone(),
            from_device_name: artifact.from_device_name.clone(),
            label: artifact.label.clone(),
            summary: artifact.summary.clone(),
            file_name: artifact.file_name.clone(),
            received_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
            context,
        };
        let bytes = serde_json::to_vec_pretty(&record).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("lukewarm inbox serialize error: {err}"),
            )
        })?;
        fs::write(&path, bytes)?;
        self.refresh_received_lukewarm_inbox();
        self.selected_received_lukewarm = Some(path.clone());
        self.networking_status = Some(format!(
            "Received luke warm context from {} and saved it to the inbox.",
            artifact.from_device_name
        ));
        Ok(path)
    }

    fn accept_received_lukewarm_context(&mut self, path: &Path) -> io::Result<PathBuf> {
        let record = read_received_lukewarm_record(path)?;
        let dir = self.applied_lukewarm_dir();
        fs::create_dir_all(&dir)?;

        for existing in load_applied_lukewarm_contexts(&self.base_path).unwrap_or_default() {
            if existing.record.from_device_id.trim() == record.from_device_id.trim()
                && existing.path != path
            {
                let _ = fs::remove_file(existing.path);
            }
        }

        let dest = dir.join(format!(
            "{}__{}.json",
            slugify_filename(
                if record.from_device_name.trim().is_empty() {
                    &record.from_device_id
                } else {
                    &record.from_device_name
                },
                "peer"
            ),
            slugify_filename(
                if record.context.label.trim().is_empty() {
                    "lukewarm_context"
                } else {
                    record.context.label.trim()
                },
                "lukewarm_context"
            )
        ));
        let bytes = serde_json::to_vec_pretty(&record).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("lukewarm apply serialize error: {err}"),
            )
        })?;
        fs::write(&dest, bytes)?;
        fs::remove_file(path)?;
        self.refresh_received_lukewarm_inbox();
        self.networking_status = Some(format!(
            "Applied shared luke warm context from {}.",
            record.from_device_name
        ));
        if let Some(bookkeeper) = &self.bookkeeper {
            bookkeeper.append_event(
                "networking",
                "LAN",
                &format!(
                    "Applied shared luke warm context from {}.",
                    record.from_device_name
                ),
                Some(if record.summary.trim().is_empty() {
                    record.context.summary.clone()
                } else {
                    record.summary.clone()
                }),
            );
        }
        self.pulse_ecg(
            18.0,
            &format!("Applied shared luke warm from {}.", record.from_device_name),
        );
        Ok(dest)
    }

    fn dismiss_received_lukewarm_context(&mut self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)?;
        self.refresh_received_lukewarm_inbox();
        self.networking_status = Some("Dismissed a received luke warm context.".to_string());
        Ok(())
    }

    fn render_received_lukewarm_inbox(&mut self, ui: &mut egui::Ui, heading: &str) {
        self.sync_selected_received_lukewarm();
        ui.heading(heading);
        ui.label(
            "Shared luke warm context lands here first so you can preview it before making it part of this device's network-aware memory context.",
        );
        ui.horizontal(|ui| {
            if ui.button("Refresh inbox").clicked() {
                self.refresh_received_lukewarm_inbox();
            }
            let applied_count = load_applied_lukewarm_contexts(&self.base_path)
                .unwrap_or_default()
                .len();
            ui.small(format!(
                "{} waiting | {} applied",
                self.received_lukewarm_inbox.len(),
                applied_count
            ));
        });

        if self.received_lukewarm_inbox.is_empty() {
            ui.small("No shared luke warm context is waiting right now.");
            return;
        }

        let selected_path = self.selected_received_lukewarm.clone().or_else(|| {
            self.received_lukewarm_inbox
                .first()
                .map(|item| item.path.clone())
        });
        let selected_item = selected_path.as_ref().and_then(|path| {
            self.received_lukewarm_inbox
                .iter()
                .find(|item| &item.path == path)
                .cloned()
        });

        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                ScrollArea::vertical()
                    .id_source((heading, "lukewarm_inbox_list"))
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for item in &self.received_lukewarm_inbox {
                            let selected = self
                                .selected_received_lukewarm
                                .as_ref()
                                .is_some_and(|path| path == &item.path);
                            let title = if item.record.label.trim().is_empty() {
                                item.record.context.label.clone()
                            } else {
                                item.record.label.clone()
                            };
                            ui.group(|ui| {
                                if ui
                                    .selectable_label(selected, title)
                                    .on_hover_text(item.path.display().to_string())
                                    .clicked()
                                {
                                    self.selected_received_lukewarm = Some(item.path.clone());
                                }
                                ui.small(format!("from {}", item.record.from_device_name));
                                if !item.record.context.summary.trim().is_empty() {
                                    ui.small(Self::clip_chars(item.record.context.summary.trim(), 120));
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
            });

            cols[1].vertical(|ui| {
                if let Some(item) = selected_item {
                    let record = item.record.clone();
                    let path = item.path.clone();
                    let title = if record.label.trim().is_empty() {
                        record.context.label.clone()
                    } else {
                        record.label.clone()
                    };
                    ui.label(RichText::new(title).strong());
                    ui.small(format!(
                        "From {} ({}) | Source app: {}",
                        record.from_device_name,
                        record.from_device_id,
                        record.context.source_app
                    ));
                    if !record.summary.trim().is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Summary").strong());
                        ui.label(record.summary.trim());
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Apply to shared memory").clicked() {
                            if let Err(err) = self.accept_received_lukewarm_context(&path) {
                                self.networking_status = Some(format!(
                                    "Could not apply the received luke warm context: {}",
                                    err
                                ));
                            }
                        }
                        if ui.button("Dismiss").clicked() {
                            if let Err(err) = self.dismiss_received_lukewarm_context(&path) {
                                self.networking_status = Some(format!(
                                    "Could not dismiss the received luke warm context: {}",
                                    err
                                ));
                            }
                        }
                        if ui.button("Open file").clicked() {
                            open_path_in_explorer(&path);
                        }
                    });
                    if !self.settings.network_allow_shared_lukewarm_context {
                        ui.colored_label(
                            egui::Color32::from_rgb(160, 90, 40),
                            "Shared luke warm context is currently stored but not injected into prompts because `Allow shared luke warm context` is turned off.",
                        );
                    }
                    ui.add_space(8.0);
                    let mut preview = record.context.context_text.clone();
                    ScrollArea::vertical()
                        .id_source((heading, "lukewarm_inbox_preview"))
                        .max_height(220.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut preview)
                                    .desired_rows(10)
                                    .interactive(false),
                            );
                        });
                } else {
                    ui.small("Select a received luke warm context to preview it.");
                }
            });
        });
    }

    fn store_received_generic_transfer(
        &mut self,
        artifact: &ReceivedArtifact,
    ) -> io::Result<PathBuf> {
        let payload_bytes = artifact.decoded_bytes().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "transfer payload could not be decoded",
            )
        })?;

        let inbox_dir = self.received_transfer_inbox_dir();
        let payload_dir = self.received_transfer_payload_dir();
        fs::create_dir_all(&inbox_dir)?;
        fs::create_dir_all(&payload_dir)?;

        let safe_sender = slugify_filename(&artifact.from_device_name, "peer");
        let safe_label = slugify_filename(
            if artifact.label.trim().is_empty() {
                "network_transfer"
            } else {
                artifact.label.trim()
            },
            "network_transfer",
        );
        let stamp = Utc::now().timestamp_millis().max(0) as u64;
        let payload_file_name = format!(
            "{}__{}__{}.{}",
            safe_sender,
            safe_label,
            stamp,
            infer_transfer_extension(
                &artifact.file_name,
                &artifact.content_type,
                artifact.is_binary(),
            )
        );
        let payload_path = payload_dir.join(&payload_file_name);
        fs::write(&payload_path, &payload_bytes)?;

        let record_path =
            inbox_dir.join(format!("{}__{}__{}.json", safe_sender, safe_label, stamp));
        let mut record = ReceivedGenericTransferRecord {
            artifact_id: artifact.artifact_id.clone(),
            from_device_id: artifact.from_device_id.clone(),
            from_device_name: artifact.from_device_name.clone(),
            label: artifact.label.clone(),
            summary: artifact.summary.clone(),
            kind: artifact.kind.clone(),
            module_id: artifact.module_id.clone(),
            file_name: artifact.file_name.clone(),
            content_type: artifact.content_type.clone(),
            transfer_encoding: artifact.transfer_encoding.clone(),
            byte_len: artifact.byte_len,
            chunk_count: artifact.chunk_count,
            received_at_unix_ms: stamp,
            binary: artifact.is_binary(),
            payload_file_name,
            preview_text: if artifact.is_binary() {
                String::new()
            } else {
                clip_string_for_preview(&artifact.text, 4_000)
            },
            delivered_lanes: Vec::new(),
        };
        let auto_delivered_path = if record.module_id.trim().is_empty() {
            None
        } else {
            let mut lanes = self
                .matching_module_asset_lanes_for_transfer(
                    &record.module_id,
                    &record.kind,
                    &record.content_type,
                    record.byte_len,
                )
                .into_iter()
                .filter(|lane| {
                    lane.delivery_mode == crate::modules::ModuleAssetDeliveryMode::BridgeInbox
                })
                .collect::<Vec<_>>();
            if lanes.len() == 1 {
                Some(self.deliver_generic_transfer_record_to_lane(
                    &mut record,
                    &payload_bytes,
                    &lanes.remove(0),
                )?)
            } else {
                None
            }
        };
        self.persist_received_generic_transfer_record(&record_path, &record)?;
        self.refresh_received_transfer_inbox();
        self.selected_received_transfer = Some(record_path.clone());
        self.networking_status = Some(if let Some(delivered_path) = auto_delivered_path {
            let lane_label = record
                .delivered_lanes
                .first()
                .map(|lane| lane.lane_label.as_str())
                .unwrap_or("module lane");
            format!(
                "Received `{}` from {} and delivered it into {} at {}.",
                if artifact.label.trim().is_empty() {
                    artifact.kind.trim()
                } else {
                    artifact.label.trim()
                },
                artifact.from_device_name,
                lane_label,
                delivered_path.display()
            )
        } else {
            format!(
                "Received `{}` from {} and saved it to the transfer inbox.",
                if artifact.label.trim().is_empty() {
                    artifact.kind.trim()
                } else {
                    artifact.label.trim()
                },
                artifact.from_device_name
            )
        });
        Ok(record_path)
    }

    fn accept_received_generic_transfer(&mut self, path: &Path) -> io::Result<PathBuf> {
        let record = read_received_generic_transfer_record(path)?;
        let payload_path = self
            .received_transfer_payload_dir()
            .join(record.payload_file_name.clone());
        if !payload_path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("transfer payload missing: {}", payload_path.display()),
            ));
        }

        let dest_dir = self.applied_transfer_dir();
        fs::create_dir_all(&dest_dir)?;
        let dest_file_name = if record.file_name.trim().is_empty() {
            record.payload_file_name.clone()
        } else {
            format!(
                "{}__{}",
                slugify_filename(&record.from_device_name, "peer"),
                sanitize_filename_keep_extension(&record.file_name)
            )
        };
        let dest_path = unique_path_in_dir(&dest_dir, &dest_file_name);
        fs::copy(&payload_path, &dest_path)?;

        let sidecar_path = dest_dir.join(format!(
            "{}.meta.json",
            dest_path.file_name().unwrap_or_default().to_string_lossy()
        ));
        let sidecar = serde_json::to_vec_pretty(&record).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("generic transfer meta serialize error: {err}"),
            )
        })?;
        fs::write(&sidecar_path, sidecar)?;
        let _ = fs::remove_file(&payload_path);
        fs::remove_file(path)?;
        self.refresh_received_transfer_inbox();
        self.networking_status = Some(format!(
            "Imported `{}` from {} into {}.",
            if record.label.trim().is_empty() {
                record.kind.trim()
            } else {
                record.label.trim()
            },
            record.from_device_name,
            dest_path.display()
        ));
        self.pulse_ecg(18.0, "Imported a received file-style transfer.");
        Ok(dest_path)
    }

    fn dismiss_received_generic_transfer(&mut self, path: &Path) -> io::Result<()> {
        let record = read_received_generic_transfer_record(path)?;
        let payload_path = self
            .received_transfer_payload_dir()
            .join(record.payload_file_name);
        let _ = fs::remove_file(payload_path);
        fs::remove_file(path)?;
        self.refresh_received_transfer_inbox();
        self.networking_status = Some("Dismissed a received file-style transfer.".to_string());
        Ok(())
    }

    fn render_received_generic_transfer_inbox(&mut self, ui: &mut egui::Ui, heading: &str) {
        self.sync_selected_received_transfer();
        ui.heading(heading);
        ui.label(
            "Unknown, file-style, or binary transfers land here first so you can inspect them before importing them into this machine.",
        );
        ui.horizontal(|ui| {
            if ui.button("Refresh inbox").clicked() {
                self.refresh_received_transfer_inbox();
            }
            ui.small(format!(
                "{} transfer(s) waiting",
                self.received_transfer_inbox.len()
            ));
        });

        if self.received_transfer_inbox.is_empty() {
            ui.small("No generic file-style transfers are waiting right now.");
            return;
        }

        let selected_item = self
            .selected_received_transfer
            .as_ref()
            .and_then(|path| {
                self.received_transfer_inbox
                    .iter()
                    .find(|item| &item.path == path)
                    .cloned()
            })
            .or_else(|| self.received_transfer_inbox.first().cloned());

        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                ScrollArea::vertical()
                    .id_source((heading, "generic_transfer_inbox_list"))
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for item in &self.received_transfer_inbox {
                            let selected = self
                                .selected_received_transfer
                                .as_ref()
                                .is_some_and(|path| path == &item.path);
                            let title = if item.record.label.trim().is_empty() {
                                item.record.kind.clone()
                            } else {
                                item.record.label.clone()
                            };
                            ui.group(|ui| {
                                if ui
                                    .selectable_label(selected, title)
                                    .on_hover_text(item.path.display().to_string())
                                    .clicked()
                                {
                                    self.selected_received_transfer = Some(item.path.clone());
                                }
                                ui.small(format!(
                                    "{} | {}",
                                    item.record.from_device_name,
                                    format_network_transfer_meta(
                                        &item.record.content_type,
                                        &item.record.transfer_encoding,
                                        item.record.byte_len,
                                        item.record.chunk_count,
                                    )
                                ));
                                if !item.record.summary.trim().is_empty() {
                                    ui.small(Self::clip_chars(item.record.summary.trim(), 120));
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
            });

            cols[1].vertical(|ui| {
                if let Some(item) = selected_item {
                    let record = item.record.clone();
                    let path = item.path.clone();
                    let payload_path = self
                        .received_transfer_payload_dir()
                        .join(record.payload_file_name.clone());
                    let module = self.module_by_id(&record.module_id);
                    let matching_lanes = if record.module_id.trim().is_empty() {
                        Vec::new()
                    } else {
                        self.matching_module_asset_lanes_for_transfer(
                            &record.module_id,
                            &record.kind,
                            &record.content_type,
                            record.byte_len,
                        )
                    };
                    ui.label(
                        RichText::new(if record.label.trim().is_empty() {
                            record.kind.clone()
                        } else {
                            record.label.clone()
                        })
                        .strong(),
                    );
                    ui.small(format!(
                        "From {} ({})",
                        record.from_device_name, record.from_device_id
                    ));
                    if !record.module_id.trim().is_empty() {
                        ui.small(format!("Module: {}", record.module_id));
                    }
                    if !record.file_name.trim().is_empty() {
                        ui.small(format!("Original file: {}", record.file_name));
                    }
                    ui.small(format_network_transfer_meta(
                        &record.content_type,
                        &record.transfer_encoding,
                        record.byte_len,
                        record.chunk_count,
                    ));
                    ui.small(format!("Inbox record: {}", path.display()));
                    ui.small(format!("Payload file: {}", payload_path.display()));
                    if !record.summary.trim().is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Summary").strong());
                        ui.label(record.summary.trim());
                    }
                    if !record.module_id.trim().is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Module asset lanes").strong());
                        if !matching_lanes.is_empty() {
                            ui.small(format!(
                                "{} declared {} matching incoming lane(s) for this transfer.",
                                module
                                    .as_ref()
                                    .map(|module| module.manifest.title.as_str())
                                    .unwrap_or(record.module_id.as_str()),
                                matching_lanes.len()
                            ));
                            for lane in &matching_lanes {
                                let delivered = record
                                    .delivered_lanes
                                    .iter()
                                    .find(|entry| entry.lane_id.trim() == lane.lane_id.trim());
                                ui.group(|ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.strong(lane.label.trim());
                                        ui.small(format!(
                                            "[{} | {}]",
                                            lane.lane_id,
                                            lane.delivery_mode.label()
                                        ));
                                    });
                                    let mut meta = vec![lane.direction.label().to_string()];
                                    if !lane.artifact_kinds.is_empty() {
                                        meta.push(format!("kinds: {}", lane.artifact_kinds.join(", ")));
                                    }
                                    if !lane.accepted_content_types.is_empty() {
                                        meta.push(format!(
                                            "content: {}",
                                            lane.accepted_content_types.join(", ")
                                        ));
                                    }
                                    if let Some(max_bytes) = lane.max_bytes {
                                        meta.push(format!(
                                            "max {}",
                                            format_network_transfer_size(max_bytes)
                                        ));
                                    }
                                    if lane.replayable {
                                        meta.push("replayable".to_string());
                                    }
                                    ui.small(meta.join(" | "));
                                    if let Some(delivered) = delivered {
                                        ui.small(format!(
                                            "Delivered here at {} -> {}",
                                            delivered.delivered_at_unix_ms,
                                            delivered.bridge_record_path
                                        ));
                                    }
                                    ui.horizontal_wrapped(|ui| {
                                        let button_label = if delivered.is_some() {
                                            "Re-deliver to lane"
                                        } else {
                                            "Deliver to lane"
                                        };
                                        if ui.button(button_label).clicked() {
                                            if let Err(err) = self
                                                .deliver_received_generic_transfer_to_lane(
                                                    &path,
                                                    &lane.lane_id,
                                                )
                                            {
                                                self.networking_status = Some(format!(
                                                    "Could not deliver that transfer to {}: {}",
                                                    lane.label, err
                                                ));
                                            }
                                        }
                                        if let Some(module) = &module {
                                            if ui.button("Open lane").clicked() {
                                                open_path_in_explorer(&bridge_incoming_asset_lane_dir(
                                                    &module.folder,
                                                    &lane.lane_id,
                                                ));
                                            }
                                        }
                                    });
                                    for note in &lane.notes {
                                        ui.small(format!("Note: {}", note));
                                    }
                                });
                                ui.add_space(4.0);
                            }
                        } else {
                            ui.small(
                                "This module did not declare a matching incoming asset lane for this transfer, so Chatty-EDU is keeping it in the generic inbox.",
                            );
                        }
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Import to local files").clicked() {
                            if let Err(err) = self.accept_received_generic_transfer(&path) {
                                self.networking_status =
                                    Some(format!("Could not import that transfer: {}", err));
                            }
                        }
                        if ui.button("Dismiss").clicked() {
                            if let Err(err) = self.dismiss_received_generic_transfer(&path) {
                                self.networking_status =
                                    Some(format!("Could not dismiss that transfer: {}", err));
                            }
                        }
                        if ui.button("Open payload").clicked() {
                            open_path_in_explorer(&payload_path);
                        }
                        if ui.button("Open imports").clicked() {
                            open_path_in_explorer(&self.applied_transfer_dir());
                        }
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("Preview").strong());
                    if record.binary {
                        ui.small(
                            "This transfer is binary/file-style, so Chatty-EDU is only showing metadata here. Import it to the local files area or open the payload directly.",
                        );
                    } else {
                        let mut preview = record.preview_text.clone();
                        ui.add(
                            egui::TextEdit::multiline(&mut preview)
                                .desired_width(f32::INFINITY)
                                .desired_rows(12)
                                .interactive(false),
                        );
                    }
                } else {
                    ui.small("Select a received transfer to preview it.");
                }
            });
        });
    }

    fn store_received_workflow_bundle(
        &mut self,
        artifact: &ReceivedArtifact,
    ) -> io::Result<PathBuf> {
        let bundle: WorkflowBundle = serde_json::from_str(&artifact.text).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("workflow bundle parse error: {err}"),
            )
        })?;

        let dir = self.received_bundle_inbox_dir();
        fs::create_dir_all(&dir)?;
        let safe_sender = slugify_filename(&artifact.from_device_name, "peer");
        let safe_label = slugify_filename(
            if artifact.label.trim().is_empty() {
                "workflow_bundle"
            } else {
                artifact.label.trim()
            },
            "workflow_bundle",
        );
        let file_name = format!(
            "workflow_bundle_{}_{}_{}.json",
            safe_sender,
            safe_label,
            Utc::now().format("%Y%m%d_%H%M%S")
        );
        let path = dir.join(file_name);

        let record = ReceivedWorkflowBundleRecord {
            artifact_id: artifact.artifact_id.clone(),
            from_device_id: artifact.from_device_id.clone(),
            from_device_name: artifact.from_device_name.clone(),
            label: artifact.label.clone(),
            summary: artifact.summary.clone(),
            file_name: artifact.file_name.clone(),
            received_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
            bundle,
        };
        let bytes = serde_json::to_vec_pretty(&record).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("workflow bundle serialize error: {err}"),
            )
        })?;
        fs::write(&path, bytes)?;
        self.refresh_received_bundle_inbox();
        self.selected_received_bundle = Some(path.clone());
        self.networking_status = Some(format!(
            "Received a classroom setup bundle from {} and saved it to the inbox.",
            artifact.from_device_name
        ));
        Ok(path)
    }

    fn persist_received_generic_transfer_record(
        &self,
        path: &Path,
        record: &ReceivedGenericTransferRecord,
    ) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(record).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("generic transfer serialize error: {err}"),
            )
        })?;
        fs::write(path, bytes)
    }

    fn matching_module_asset_lanes_for_transfer(
        &self,
        module_id: &str,
        kind: &str,
        content_type: &str,
        byte_len: u64,
    ) -> Vec<ModuleNetworkAssetLane> {
        self.module_by_id(module_id)
            .and_then(|module| module.manifest.network_capabilities)
            .map(|caps| {
                caps.matching_receive_asset_lanes(kind, content_type, byte_len)
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    fn deliver_generic_transfer_record_to_lane(
        &mut self,
        record: &mut ReceivedGenericTransferRecord,
        payload_bytes: &[u8],
        lane: &ModuleNetworkAssetLane,
    ) -> io::Result<PathBuf> {
        let module = self.module_by_id(&record.module_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("module `{}` is not available", record.module_id),
            )
        })?;
        let delivered_at_unix_ms = Utc::now().timestamp_millis().max(0) as u64;
        let bridge_record = ModuleBridgeIncomingAssetRecord {
            asset_id: format!(
                "{}-{}",
                record.artifact_id.trim(),
                slugify_filename(&lane.lane_id, "lane")
            ),
            artifact_id: record.artifact_id.clone(),
            module_id: record.module_id.clone(),
            lane_id: lane.lane_id.clone(),
            lane_label: lane.label.clone(),
            kind: record.kind.clone(),
            label: record.label.clone(),
            summary: record.summary.clone(),
            file_name: record.file_name.clone(),
            content_type: record.content_type.clone(),
            transfer_encoding: record.transfer_encoding.clone(),
            byte_len: record.byte_len,
            chunk_count: record.chunk_count,
            binary: record.binary,
            from_device_id: record.from_device_id.clone(),
            from_device_name: record.from_device_name.clone(),
            delivered_at_unix_ms,
            payload_file_name: record.payload_file_name.clone(),
        };
        let bridge_record_path = write_bridge_incoming_asset(
            &module.folder,
            &lane.lane_id,
            &bridge_record,
            payload_bytes,
        )
        .map_err(|err| io::Error::other(err.to_string()))?;
        record.delivered_lanes.retain(|entry| {
            !entry
                .lane_id
                .trim()
                .eq_ignore_ascii_case(lane.lane_id.trim())
        });
        record
            .delivered_lanes
            .push(ReceivedGenericTransferLaneDelivery {
                lane_id: lane.lane_id.clone(),
                lane_label: lane.label.clone(),
                delivered_at_unix_ms,
                bridge_record_path: bridge_record_path.display().to_string(),
            });
        record
            .delivered_lanes
            .sort_by(|left, right| right.delivered_at_unix_ms.cmp(&left.delivered_at_unix_ms));
        if lane.replayable {
            self.remember_recoverable_module_asset(
                &record.kind,
                if record.label.trim().is_empty() {
                    &lane.label
                } else {
                    &record.label
                },
                &record.module_id,
                if record.summary.trim().is_empty() {
                    &lane.label
                } else {
                    &record.summary
                },
                if record.file_name.trim().is_empty() {
                    &record.payload_file_name
                } else {
                    &record.file_name
                },
                &record.content_type,
                payload_bytes,
                record.binary,
            );
        }
        Ok(bridge_record_path)
    }

    fn deliver_received_generic_transfer_to_lane(
        &mut self,
        path: &Path,
        lane_id: &str,
    ) -> io::Result<PathBuf> {
        let mut record = read_received_generic_transfer_record(path)?;
        if record.module_id.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "this transfer is not scoped to a module",
            ));
        }
        let lane = self
            .matching_module_asset_lanes_for_transfer(
                &record.module_id,
                &record.kind,
                &record.content_type,
                record.byte_len,
            )
            .into_iter()
            .find(|lane| lane.lane_id.trim() == lane_id.trim())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "no matching incoming asset lane `{}` is declared for {}",
                        lane_id, record.module_id
                    ),
                )
            })?;
        let payload_path = self
            .received_transfer_payload_dir()
            .join(record.payload_file_name.clone());
        if !payload_path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("transfer payload missing: {}", payload_path.display()),
            ));
        }
        let payload_bytes = fs::read(&payload_path)?;
        let bridge_record_path =
            self.deliver_generic_transfer_record_to_lane(&mut record, &payload_bytes, &lane)?;
        self.persist_received_generic_transfer_record(path, &record)?;
        self.refresh_received_transfer_inbox();
        self.selected_received_transfer = Some(path.to_path_buf());
        self.networking_status = Some(format!(
            "Delivered `{}` from {} into {} -> {}.",
            if record.label.trim().is_empty() {
                record.kind.trim()
            } else {
                record.label.trim()
            },
            record.from_device_name,
            lane.label.trim(),
            bridge_record_path.display()
        ));
        self.pulse_ecg(16.0, "Delivered a network transfer into a module lane.");
        Ok(bridge_record_path)
    }

    fn accept_received_workflow_bundle(&mut self, path: &Path) -> io::Result<()> {
        let record = read_received_workflow_bundle_record(path)?;
        let bundle = record.bundle.clone();
        let mut notes = Vec::new();

        self.settings.teacher_mode = bundle.teacher_mode.clone();
        self.settings.default_year_level = bundle.default_year_level.clone();
        self.settings.homework_hints_only = bundle.homework_hints_only;
        self.settings.janet = bundle.janet.clone();
        self.settings.voice = bundle.voice.clone();
        self.settings.game = bundle.game.clone();
        self.settings.model.max_tokens = bundle.model_max_tokens.max(32);

        if let Some(path) = self.resolve_portable_model_hint(bundle.model_hint.as_deref()) {
            self.settings.model.path = path.to_string_lossy().to_string();
            self.settings.model.name = if bundle.model_name.trim().is_empty() {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("model")
                    .to_string()
            } else {
                bundle.model_name.clone()
            };
            notes.push(format!(
                "main model -> {}",
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string())
            ));
        } else if let Some(hint) = bundle.model_hint.as_deref() {
            notes.push(format!("main model missing locally ({hint})"));
        }

        if let Some(path) =
            self.resolve_portable_model_hint(bundle.bookkeeper_model_hint.as_deref())
        {
            self.settings.bookkeeper_model_path = path.to_string_lossy().to_string();
            self.settings.bookkeeper_model_name = if bundle.bookkeeper_model_name.trim().is_empty()
            {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("bookkeeper")
                    .to_string()
            } else {
                bundle.bookkeeper_model_name.clone()
            };
            notes.push(format!(
                "bookkeeper -> {}",
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string())
            ));
        } else if bundle.bookkeeper_model_hint.is_none() {
            self.settings.bookkeeper_model_name = bundle.bookkeeper_model_name.clone();
            self.settings.bookkeeper_model_path.clear();
            if !bundle.bookkeeper_model_name.trim().is_empty() {
                notes.push(format!(
                    "bookkeeper mode -> {}",
                    bundle.bookkeeper_model_name
                ));
            }
        } else if let Some(hint) = bundle.bookkeeper_model_hint.as_deref() {
            notes.push(format!("bookkeeper model missing locally ({hint})"));
        }

        save_settings(&self.settings, &self.base_path)?;
        self.available_models = discover_local_models(&self.base_path);
        self.resync_homework();
        self.resync_revision();

        fs::remove_file(path)?;
        self.refresh_received_bundle_inbox();

        let summary_line = if record.summary.trim().is_empty() {
            bundle.summary.trim()
        } else {
            record.summary.trim()
        };
        self.networking_status = Some(if notes.is_empty() {
            format!(
                "Applied classroom setup bundle from {}.",
                record.from_device_name
            )
        } else {
            format!(
                "Applied classroom setup bundle from {} ({})",
                record.from_device_name,
                notes.join(" | ")
            )
        });
        if let Some(bookkeeper) = &self.bookkeeper {
            bookkeeper.append_event(
                "networking",
                "LAN",
                &format!(
                    "Applied classroom setup bundle from {}.\n\nSummary: {}\nNotes: {}",
                    record.from_device_name,
                    if summary_line.is_empty() {
                        "(no summary)".to_string()
                    } else {
                        summary_line.to_string()
                    },
                    if notes.is_empty() {
                        "(no model remap notes)".to_string()
                    } else {
                        notes.join(" | ")
                    }
                ),
                Some(format!(
                    "Teacher mode: {} | Year level: {}",
                    bundle.teacher_mode, bundle.default_year_level
                )),
            );
        }
        self.pulse_ecg(20.0, "Applied a classroom setup bundle from the network.");
        Ok(())
    }

    fn dismiss_received_workflow_bundle(&mut self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)?;
        self.refresh_received_bundle_inbox();
        self.networking_status = Some("Dismissed a received classroom setup bundle.".to_string());
        Ok(())
    }

    fn render_received_revision_pack_inbox(&mut self, ui: &mut egui::Ui, heading: &str) {
        self.sync_selected_received_revision_pack();
        ui.heading(heading);
        ui.label(
            "Network-delivered revision packs land here first so you can preview them before bringing them into this device's revision workspace.",
        );
        ui.horizontal(|ui| {
            if ui.button("Refresh inbox").clicked() {
                self.refresh_received_revision_inbox();
            }
            ui.small(format!(
                "{} pack(s) waiting",
                self.received_revision_inbox.len()
            ));
        });

        if self.received_revision_inbox.is_empty() {
            ui.small("No received revision packs are waiting right now.");
            return;
        }

        self.sync_selected_received_revision_pack();
        let selected_path = self.selected_received_revision_pack.clone().or_else(|| {
            self.received_revision_inbox
                .first()
                .map(|item| item.path.clone())
        });
        let selected_item = selected_path.as_ref().and_then(|path| {
            self.received_revision_inbox
                .iter()
                .find(|item| &item.path == path)
                .cloned()
        });

        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                ScrollArea::vertical()
                    .id_source((heading, "received_revision_inbox_list"))
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for item in &self.received_revision_inbox {
                            let selected = self
                                .selected_received_revision_pack
                                .as_ref()
                                .is_some_and(|path| path == &item.path);
                            let title = if item.record.label.trim().is_empty() {
                                "Revision pack".to_string()
                            } else {
                                item.record.label.clone()
                            };
                            ui.group(|ui| {
                                if ui
                                    .selectable_label(selected, title)
                                    .on_hover_text(item.path.display().to_string())
                                    .clicked()
                                {
                                    self.selected_received_revision_pack = Some(item.path.clone());
                                }
                                ui.small(format!("From {}", item.record.from_device_name));
                                if !item.record.summary.trim().is_empty() {
                                    ui.small(Self::clip_chars(item.record.summary.trim(), 120));
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
            });

            cols[1].vertical(|ui| {
                if let Some(item) = selected_item {
                    let record = item.record.clone();
                    let path = item.path.clone();
                    let title = if record.label.trim().is_empty() {
                        "Revision pack".to_string()
                    } else {
                        record.label.clone()
                    };
                    ui.label(RichText::new(title).strong());
                    ui.small(format!(
                        "From {} ({})",
                        record.from_device_name, record.from_device_id
                    ));
                    if !record.summary.trim().is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Teacher note").strong());
                        ui.label(record.summary.trim());
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Apply pack now").clicked() {
                            if let Err(err) = self.accept_received_revision_pack(&path) {
                                self.networking_status = Some(format!(
                                    "Could not apply the received revision pack: {}",
                                    err
                                ));
                            }
                        }
                        if ui.button("Dismiss").clicked() {
                            if let Err(err) = self.dismiss_received_revision_pack(&path) {
                                self.networking_status = Some(format!(
                                    "Could not dismiss the received revision pack: {}",
                                    err
                                ));
                            }
                        }
                        if ui.button("Open file").clicked() {
                            open_path_in_explorer(&path);
                        }
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("Pack preview").strong());
                    let mut preview = record.markdown.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut preview)
                            .desired_width(f32::INFINITY)
                            .desired_rows(14)
                            .interactive(false),
                    );
                } else {
                    ui.small("Select a received revision pack to preview it.");
                }
            });
        });
    }

    fn render_received_workflow_bundle_inbox(&mut self, ui: &mut egui::Ui, heading: &str) {
        self.sync_selected_received_bundle();
        ui.heading(heading);
        ui.label(
            "Shared classroom setup bundles land here first so you can preview them before applying them to this Chatty-EDU instance.",
        );
        ui.horizontal(|ui| {
            if ui.button("Refresh inbox").clicked() {
                self.refresh_received_bundle_inbox();
            }
            ui.small(format!(
                "{} bundle(s) waiting",
                self.received_bundle_inbox.len()
            ));
        });

        if self.received_bundle_inbox.is_empty() {
            ui.small("No received classroom setup bundles are waiting right now.");
            return;
        }

        let selected_item = self
            .selected_received_bundle
            .as_ref()
            .and_then(|path| {
                self.received_bundle_inbox
                    .iter()
                    .find(|item| &item.path == path)
                    .cloned()
            })
            .or_else(|| self.received_bundle_inbox.first().cloned());

        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                ScrollArea::vertical()
                    .id_source((heading, "received_bundle_inbox_list"))
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for item in &self.received_bundle_inbox {
                            let selected = self
                                .selected_received_bundle
                                .as_ref()
                                .is_some_and(|path| path == &item.path);
                            let title = if item.record.label.trim().is_empty() {
                                "Classroom setup".to_string()
                            } else {
                                item.record.label.clone()
                            };
                            let summary = if item.record.summary.trim().is_empty() {
                                item.record.bundle.summary.trim()
                            } else {
                                item.record.summary.trim()
                            };
                            ui.group(|ui| {
                                if ui
                                    .selectable_label(selected, title)
                                    .on_hover_text(item.path.display().to_string())
                                    .clicked()
                                {
                                    self.selected_received_bundle = Some(item.path.clone());
                                }
                                ui.small(format!("From {}", item.record.from_device_name));
                                if !summary.is_empty() {
                                    ui.small(Self::clip_chars(summary, 120));
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
            });

            cols[1].vertical(|ui| {
                if let Some(item) = selected_item {
                    let record = item.record.clone();
                    let path = item.path.clone();
                    let title = if record.label.trim().is_empty() {
                        "Classroom setup".to_string()
                    } else {
                        record.label.clone()
                    };
                    let bundle = record.bundle.clone();
                    let summary = if record.summary.trim().is_empty() {
                        bundle.summary.trim()
                    } else {
                        record.summary.trim()
                    };

                    ui.label(RichText::new(title).strong());
                    ui.small(format!(
                        "From {} ({})",
                        record.from_device_name, record.from_device_id
                    ));
                    if !summary.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Summary").strong());
                        ui.label(summary);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Apply bundle now").clicked() {
                            if let Err(err) = self.accept_received_workflow_bundle(&path) {
                                self.networking_status = Some(format!(
                                    "Could not apply the received setup bundle: {}",
                                    err
                                ));
                            }
                        }
                        if ui.button("Dismiss").clicked() {
                            if let Err(err) = self.dismiss_received_workflow_bundle(&path) {
                                self.networking_status = Some(format!(
                                    "Could not dismiss the received setup bundle: {}",
                                    err
                                ));
                            }
                        }
                        if ui.button("Open file").clicked() {
                            open_path_in_explorer(&path);
                        }
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("Bundle preview").strong());
                    ui.small(format!(
                        "Teacher mode: {} | Default year: {}",
                        bundle.teacher_mode, bundle.default_year_level
                    ));
                    ui.small(format!(
                        "Homework hints only: {} | Janet: {} | Games enabled: {} | Games in class: {}",
                        if bundle.homework_hints_only { "yes" } else { "no" },
                        if bundle.janet.enabled { "on" } else { "off" },
                        if bundle.game.enabled { "yes" } else { "no" },
                        if bundle.game.games_in_class_allowed { "yes" } else { "no" }
                    ));
                    ui.small(format!(
                        "Main model: {} | Bookkeeper: {}",
                        bundle
                            .model_hint
                            .as_deref()
                            .unwrap_or(bundle.model_name.as_str()),
                        if bundle.bookkeeper_model_hint.is_some() {
                            bundle
                                .bookkeeper_model_hint
                                .as_deref()
                                .unwrap_or(bundle.bookkeeper_model_name.as_str())
                        } else if bundle.bookkeeper_model_name.trim().is_empty() {
                            "keyword-only background summary mode"
                        } else {
                            bundle.bookkeeper_model_name.as_str()
                        }
                    ));
                    let mut preview = serde_json::to_string_pretty(&bundle).unwrap_or_default();
                    ui.add(
                        egui::TextEdit::multiline(&mut preview)
                            .desired_width(f32::INFINITY)
                            .desired_rows(14)
                            .interactive(false),
                    );
                } else {
                    ui.small("Select a received setup bundle to preview it.");
                }
            });
        });
    }

    fn store_received_module_shared_state(
        &mut self,
        artifact: &ReceivedArtifact,
    ) -> io::Result<PathBuf> {
        let module_id = artifact.module_id.trim();
        if module_id.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "module shared state is missing module_id",
            ));
        }

        let mut shared_state: ModuleBridgeSharedState = serde_json::from_str(&artifact.text)
            .map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("shared state parse error: {err}"),
                )
            })?;
        if shared_state.module_id.trim().is_empty() {
            shared_state.module_id = module_id.to_string();
        }
        if shared_state.session_id.trim().is_empty() {
            shared_state.session_id = format!("legacy-{}", artifact.artifact_id);
        }
        if shared_state.session_revision == 0 {
            shared_state.session_revision = 1;
        }
        if shared_state.authoritative_device_id.trim().is_empty() {
            shared_state.authoritative_device_id = artifact.from_device_id.clone();
        }
        if shared_state.authoritative_device_name.trim().is_empty() {
            shared_state.authoritative_device_name = artifact.from_device_name.clone();
        }
        shared_state.host_authoritative = true;

        let incoming = ModuleBridgeIncomingSharedState {
            module_id: shared_state.module_id.clone(),
            from_device_id: artifact.from_device_id.clone(),
            from_device_name: artifact.from_device_name.clone(),
            summary: if artifact.summary.trim().is_empty() {
                shared_state.summary.clone()
            } else {
                artifact.summary.clone()
            },
            session_id: shared_state.session_id.clone(),
            session_revision: shared_state.session_revision,
            authoritative_device_id: shared_state.authoritative_device_id.clone(),
            authoritative_device_name: shared_state.authoritative_device_name.clone(),
            host_authoritative: shared_state.host_authoritative,
            payload: shared_state.payload.clone(),
            received_at_unix_ms: Utc::now().timestamp_millis().max(0) as u64,
        };

        if let Some(module) = self
            .modules
            .iter()
            .find(|module| module.manifest.id == module_id)
            .cloned()
        {
            let can_receive_shared_state = module
                .manifest
                .network_capabilities
                .as_ref()
                .map(|caps| caps.has(ModuleNetworkFeature::SharedStateReceive))
                .unwrap_or(true);
            if !can_receive_shared_state {
                let dir = self.network_inbox_dir().join("module_states");
                fs::create_dir_all(&dir)?;
                let path = dir.join(format!(
                    "{}__{}__{}.json",
                    slugify_filename(module_id, "module"),
                    slugify_filename(&artifact.from_device_name, "peer"),
                    Utc::now().format("%Y%m%d_%H%M%S")
                ));
                let bytes = serde_json::to_vec_pretty(&incoming).map_err(|err| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("shared state serialize error: {err}"),
                    )
                })?;
                fs::write(&path, bytes)?;
                self.send_module_session_ack(
                    module_id,
                    &artifact.from_device_id,
                    &artifact.from_device_name,
                    &shared_state,
                    false,
                    false,
                    "Stored for later because this module does not declare shared-state receive support yet.",
                );
                self.networking_status = Some(format!(
                    "Received shared state for {} from {}. Saved to inbox because the module does not declare receive support yet.",
                    module.manifest.title, artifact.from_device_name
                ));
                return Ok(path);
            }
            if let Some(reason) = self.stale_module_state_message(&module.folder, &shared_state) {
                self.send_module_session_ack(
                    module_id,
                    &artifact.from_device_id,
                    &artifact.from_device_name,
                    &shared_state,
                    false,
                    true,
                    &reason,
                );
                self.networking_status = Some(format!(
                    "Ignored stale shared state for {} from {}.",
                    module.manifest.title, artifact.from_device_name
                ));
                return Ok(bridge_incoming_shared_state_path(&module.folder));
            }
            write_bridge_incoming_shared_state(&module.folder, &incoming)
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
            let path = bridge_incoming_shared_state_path(&module.folder);
            self.send_module_session_ack(
                module_id,
                &artifact.from_device_id,
                &artifact.from_device_name,
                &shared_state,
                true,
                false,
                &format!(
                    "Applied revision {} for session {}.",
                    shared_state.session_revision, shared_state.session_id
                ),
            );
            self.networking_status = Some(format!(
                "Received shared state for {} from {}.",
                module.manifest.title, artifact.from_device_name
            ));
            Ok(path)
        } else {
            let dir = self.network_inbox_dir().join("module_states");
            fs::create_dir_all(&dir)?;
            let path = dir.join(format!(
                "{}__{}__{}.json",
                slugify_filename(module_id, "module"),
                slugify_filename(&artifact.from_device_name, "peer"),
                Utc::now().format("%Y%m%d_%H%M%S")
            ));
            let bytes = serde_json::to_vec_pretty(&incoming).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("shared state serialize error: {err}"),
                )
            })?;
            fs::write(&path, bytes)?;
            self.send_module_session_ack(
                module_id,
                &artifact.from_device_id,
                &artifact.from_device_name,
                &shared_state,
                false,
                false,
                "Stored for later because this module is not installed here.",
            );
            self.networking_status = Some(format!(
                "Received shared state for missing module `{}` from {}. Saved to inbox.",
                module_id, artifact.from_device_name
            ));
            Ok(path)
        }
    }

    fn resync_revision(&mut self) {
        self.revision_sources = load_revision_sources(
            &self.base_path,
            Some(self.settings.student.student_id.as_str()),
        )
        .unwrap_or_default();
        self.revision_progress = load_revision_progress(&self.base_path)
            .unwrap_or_default()
            .into_iter()
            .map(|progress| (progress.revision_key.clone(), progress))
            .collect();
        let progress_map = self.revision_progress.clone();
        self.revision_sources.sort_by(|a, b| {
            let a_confidence_gap = progress_map
                .get(&a.revision_key)
                .map(|progress| 100 - progress.confidence.clamp(0, 100))
                .unwrap_or(0);
            let b_confidence_gap = progress_map
                .get(&b.revision_key)
                .map(|progress| 100 - progress.confidence.clamp(0, 100))
                .unwrap_or(0);
            let a_review_count = progress_map
                .get(&a.revision_key)
                .map(|progress| progress.review_count)
                .unwrap_or(0);
            let b_review_count = progress_map
                .get(&b.revision_key)
                .map(|progress| progress.review_count)
                .unwrap_or(0);

            b_confidence_gap
                .cmp(&a_confidence_gap)
                .then_with(|| revision_priority(b).cmp(&revision_priority(a)))
                .then_with(|| a_review_count.cmp(&b_review_count))
                .then_with(|| b.submitted_at.cmp(&a.submitted_at))
        });
        self.past_papers = load_past_papers(&self.base_path).unwrap_or_default();

        let revision_is_valid = self.selected_revision.as_ref().is_some_and(|key| {
            self.revision_sources
                .iter()
                .any(|source| &source.revision_key == key)
        });
        if !revision_is_valid {
            self.selected_revision = self
                .revision_sources
                .first()
                .map(|source| source.revision_key.clone());
        }
        self.sync_revision_editor_from_selection();
        self.refresh_received_revision_inbox();
    }

    fn sync_revision_editor_from_selection(&mut self) {
        if let Some(key) = self.selected_revision.clone() {
            if let Some(progress) = self.revision_progress.get(&key) {
                self.revision_notes = progress.notes.clone();
                self.revision_confidence = progress.confidence.clamp(0, 100);
                return;
            }
        }
        self.revision_notes.clear();
        self.revision_confidence = 50;
    }

    fn open_diagnostics_tab(&mut self) {
        self.refresh_diagnostics_report();
        self.pulse_ecg(18.0, "Opened diagnostics.");
        self.open_or_focus_tab("diagnostics", |_app| Tab {
            id: 0,
            title: "Diagnostics".to_string(),
            kind: TabKind::Diagnostics,
            closable: true,
            key: "diagnostics".to_string(),
        });
    }

    fn refresh_diagnostics_report(&mut self) {
        let audience = if self.teacher_unlocked {
            DiagnosticsAudience::Teacher
        } else {
            DiagnosticsAudience::StudentSafe
        };
        self.diagnostics_audience = audience;
        self.diagnostics_report = self.build_diagnostics_report(audience);
        self.pulse_ecg(16.0, "Refreshed the diagnostic report.");
    }

    fn pulse_ecg(&mut self, intensity: f32, note: &str) {
        self.ecg_window.record_activity(intensity, note);
    }

    fn build_diagnostics_report(&self, audience: DiagnosticsAudience) -> String {
        let yes_no = |b: bool| if b { "yes" } else { "no" };
        let now = Utc::now().to_rfc3339();
        let build = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let local_model = if cfg!(feature = "local-model") {
            "enabled"
        } else {
            "disabled"
        };

        let mut warnings: Vec<String> = Vec::new();
        let mut out = String::new();
        out.push_str("Chatty-EDU Diagnostic / Health Check\n");
        out.push_str(&format!(
            "Version: {} ({build})\n",
            env!("CARGO_PKG_VERSION")
        ));
        out.push_str(&format!("Timestamp (UTC): {now}\n"));
        out.push_str(&format!(
            "Audience: {}\n",
            match audience {
                DiagnosticsAudience::StudentSafe => "student-safe",
                DiagnosticsAudience::Teacher => "teacher",
            }
        ));
        out.push_str(&format!(
            "OS/Arch: {}/{}\n",
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        out.push_str(&format!("Local-model feature: {local_model}\n"));
        out.push_str(&format!("Base path: {}\n", self.base_path.display()));
        if let Ok(exe) = std::env::current_exe() {
            out.push_str(&format!("Executable: {}\n", exe.display()));
        }
        out.push('\n');

        let base = &self.base_path;
        let dirs: Vec<(&str, PathBuf)> = vec![
            ("config", base.join("config")),
            ("config/bookkeeper", bookkeeper_dir(base)),
            ("models", base.join("models")),
            ("homework/assigned", base.join("homework").join("assigned")),
            ("homework/outgoing", base.join("homework").join("outgoing")),
            (
                "homework/completed",
                base.join("homework").join("completed"),
            ),
            ("homework/marking", base.join("homework").join("marking")),
            (
                "homework/printables",
                base.join("homework").join("printables"),
            ),
            ("homework/rubrics", base.join("homework").join("rubrics")),
            ("modules", base.join("modules")),
            ("themes", base.join("themes")),
            ("runtime", base.join("runtime")),
            ("logs", base.join("logs")),
        ];

        out.push_str("Folders:\n");
        for (label, path) in &dirs {
            let exists = path.exists();
            if !exists {
                warnings.push(format!("Missing folder: {label}"));
            }
            out.push_str(&format!(
                "- {label}: {} ({})\n",
                if exists { "ok" } else { "missing" },
                path.display()
            ));
        }
        out.push('\n');

        // Basic writability check (best-effort).
        let write_test_path = base.join("runtime").join("health_check_write_test.tmp");
        match fs::write(&write_test_path, "ok") {
            Ok(_) => {
                let _ = fs::remove_file(&write_test_path);
                out.push_str("Write test: ok (runtime)\n\n");
            }
            Err(err) => {
                warnings.push("Base path not writable (write test failed)".to_string());
                out.push_str(&format!("Write test: FAILED ({err})\n\n"));
            }
        }

        out.push_str("Model:\n");
        out.push_str(&format!("- name: {}\n", self.settings.model.name));
        out.push_str(&format!("- path: {}\n", self.settings.model.path));
        out.push_str(&format!(
            "- max_tokens: {}\n",
            self.settings.model.max_tokens
        ));
        out.push_str(&format!(
            "- bookkeeper_role: {}\n",
            if self.settings.bookkeeper_model_name.trim().is_empty() {
                "Keyword-only background summary mode"
            } else {
                &self.settings.bookkeeper_model_name
            }
        ));
        if audience == DiagnosticsAudience::Teacher
            && !self.settings.bookkeeper_model_path.is_empty()
        {
            out.push_str(&format!(
                "- bookkeeper_path: {}\n",
                self.settings.bookkeeper_model_path
            ));
        }
        let model_path = Path::new(&self.settings.model.path);
        let model_exists = !self.settings.model.path.trim().is_empty() && model_path.exists();
        if !model_exists {
            warnings.push("Model file not found (check File -> Models)".to_string());
        }
        out.push_str(&format!("- exists: {}\n", yes_no(model_exists)));
        if model_exists {
            if let Ok(meta) = fs::metadata(model_path) {
                out.push_str(&format!("- size_bytes: {}\n", meta.len()));
            }
            let mut gguf_ok = false;
            match gguf_magic_ok(model_path) {
                Ok(true) => {
                    gguf_ok = true;
                    out.push_str("- gguf_magic: ok\n");
                }
                Ok(false) => {
                    warnings.push("Model file is not GGUF (missing GGUF magic)".to_string());
                    out.push_str("- gguf_magic: FAILED (not GGUF)\n");
                }
                Err(e) => {
                    warnings.push("Could not read model header".to_string());
                    out.push_str(&format!("- gguf_magic: error ({e})\n"));
                }
            }
            if gguf_ok {
                match gguf_metadata_summary(model_path) {
                    Ok(summary) => {
                        out.push_str(&summary);
                    }
                    Err(err) => {
                        warnings.push("Could not parse GGUF metadata".to_string());
                        out.push_str(&format!("- gguf_metadata: error ({err})\n"));
                    }
                }
            }
        }
        out.push('\n');

        out.push_str("Homework:\n");
        out.push_str(&format!(
            "- current_pack_loaded: {}\n",
            yes_no(self.current_pack.is_some())
        ));
        if let Some(pack) = &self.current_pack {
            out.push_str(&format!("- school_id: {}\n", pack.school_id));
            out.push_str(&format!("- class_id: {}\n", pack.class_id));
            out.push_str(&format!("- assignments: {}\n", pack.assignments.len()));
        } else {
            warnings.push("No homework pack loaded".to_string());
        }
        out.push_str(&format!(
            "- submissions_loaded: {}\n",
            self.submissions.len()
        ));
        out.push_str(&format!("- modules_loaded: {}\n", self.modules.len()));
        out.push('\n');

        if matches!(audience, DiagnosticsAudience::Teacher) {
            out.push_str("Teacher settings:\n");
            out.push_str(&format!(
                "- teacher_unlocked_this_session: {}\n",
                yes_no(self.teacher_unlocked)
            ));
            out.push_str(&format!("- teacher_mode: {}\n", self.settings.teacher_mode));
            out.push_str(&format!(
                "- homework_hints_only: {}\n",
                yes_no(self.settings.homework_hints_only)
            ));
            out.push_str(&format!(
                "- janet_enabled: {}\n",
                yes_no(self.settings.janet.enabled)
            ));
            out.push_str(&format!(
                "- janet_block_swears: {}\n",
                yes_no(self.settings.janet.block_swears)
            ));
            out.push_str(&format!(
                "- janet_block_mature_topics: {}\n",
                yes_no(self.settings.janet.block_mature_topics)
            ));
            out.push_str(&format!(
                "- games_enabled: {}\n",
                yes_no(self.settings.game.enabled)
            ));
            out.push_str(&format!(
                "- games_in_class_allowed: {}\n",
                yes_no(self.settings.game.games_in_class_allowed)
            ));

            let pin_is_default = self.settings.teacher_pin.trim() == "0000";
            if pin_is_default {
                warnings.push("Teacher PIN is still default (0000)".to_string());
            }
            out.push_str(&format!(
                "- teacher_pin_is_default: {}\n",
                yes_no(pin_is_default)
            ));
            out.push_str(&format!(
                "- secret_question_set: {}\n",
                yes_no(!self.settings.teacher_secret_question.trim().is_empty())
            ));
            out.push('\n');
        }

        if !warnings.is_empty() {
            out.push_str("Warnings:\n");
            for w in warnings {
                out.push_str(&format!("- {w}\n"));
            }
            out.push('\n');
        }

        out.push_str("Notes:\n");
        out.push_str("- This report omits student IDs/names and secrets by design.\n");
        out.push_str("- Copy/paste this report when asking for support.\n");

        out
    }

    fn open_or_focus_tab(&mut self, key: &str, builder: impl FnOnce(&mut Self) -> Tab) {
        if let Some(idx) = self.tabs.iter().position(|t| t.key == key) {
            self.active_tab = idx;
            return;
        }
        let mut tab = builder(self);
        tab.id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    fn close_tabs_by_key_prefix(&mut self, prefix: &str) {
        let active_key = self
            .tabs
            .get(self.active_tab)
            .map(|tab| tab.key.clone())
            .unwrap_or_default();
        self.tabs.retain(|tab| !tab.key.starts_with(prefix));
        if self.tabs.is_empty() {
            self.active_tab = 0;
            return;
        }
        if active_key.starts_with(prefix) {
            self.active_tab = 0;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
    }

    fn refresh_bookkeeper_search(&mut self) {
        let results = self
            .bookkeeper
            .as_ref()
            .map(|bookkeeper| bookkeeper.search(&self.bookkeeper_query))
            .unwrap_or_default();
        self.bookkeeper_status = Some(if self.bookkeeper_query.trim().is_empty() {
            format!("Showing {} most recent log entries.", results.len())
        } else {
            format!(
                "Showing {} match(es) for \"{}\".",
                results.len(),
                self.bookkeeper_query.trim()
            )
        });
        self.bookkeeper_results = results;
    }

    fn open_bookkeeper_tab(&mut self) {
        if !self.teacher_unlocked {
            self.teacher_pin_status =
                Some("Unlock teacher view to open bookkeeper logs.".to_string());
            return;
        }
        self.refresh_bookkeeper_search();
        self.open_or_focus_tab("bookkeeper", |_app| Tab {
            id: 0,
            title: "Bookkeeper".to_string(),
            kind: TabKind::Bookkeeper,
            closable: true,
            key: "bookkeeper".to_string(),
        });
    }

    fn open_networking_tab(&mut self) {
        self.open_or_focus_tab("networking", |_app| Tab {
            id: 0,
            title: "Networking".to_string(),
            kind: TabKind::Networking,
            closable: true,
            key: "networking".to_string(),
        });
    }

    fn open_sandbox_tab(&mut self) {
        if let Some(idx) = self.tabs.iter().position(|tab| tab.key == "sandbox") {
            self.active_tab = idx;
            return;
        }
        self.open_or_focus_tab("sandbox", |_app| Tab {
            id: 0,
            title: "Sandbox".to_string(),
            kind: TabKind::Sandbox,
            closable: true,
            key: "sandbox".to_string(),
        });
    }

    fn open_sandbox_file_in_editor(&mut self, path: &Path) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_status = "Sandbox folder not found.".to_string();
            return;
        };

        match ensure_sandbox_path_within_dir(&dir, path).and_then(|safe_path| {
            let text = if safe_path.exists() {
                sandbox_read(
                    &dir,
                    &safe_path
                        .strip_prefix(&dir)
                        .unwrap_or(&safe_path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    400_000,
                )?
            } else {
                String::new()
            };
            Ok((safe_path, text))
        }) {
            Ok((safe_path, text)) => {
                self.sandbox_selected = Some(safe_path.clone());
                self.sandbox_editor_path = Some(safe_path.clone());
                self.sandbox_last_working_path = Some(safe_path.clone());
                self.sandbox_editor_text = text;
                self.sandbox_status = format!("Opened {}", safe_path.display());
            }
            Err(err) => {
                self.sandbox_status = format!("Could not open sandbox file: {err}");
            }
        }
    }

    fn open_sandbox_file_and_focus_tab(&mut self, path: &Path) {
        self.open_sandbox_file_in_editor(path);
        self.open_sandbox_tab();
    }

    fn ensure_default_sandbox_scratchpad(&mut self) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_status = "Sandbox folder not found.".to_string();
            return;
        };
        match ensure_default_sandbox_scratchpad_file(&dir) {
            Ok(_) => {
                if self.sandbox_status.trim().is_empty() {
                    self.sandbox_status = "Scratchpad ready.".to_string();
                }
            }
            Err(err) => {
                self.sandbox_status = format!("Scratchpad setup failed: {err}");
            }
        }
    }

    fn ensure_default_sandbox_task_ledger(&mut self) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_status = "Sandbox folder not found.".to_string();
            return;
        };
        match ensure_default_sandbox_task_ledger_file(&dir) {
            Ok(_) => {
                if self.sandbox_status.trim().is_empty() {
                    self.sandbox_status = "Task ledger ready.".to_string();
                }
            }
            Err(err) => {
                self.sandbox_status = format!("Task ledger setup failed: {err}");
            }
        }
    }

    fn open_default_sandbox_scratchpad(&mut self) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_status = "Sandbox folder not found.".to_string();
            return;
        };
        match ensure_default_sandbox_scratchpad_file(&dir) {
            Ok(path) => self.open_sandbox_file_and_focus_tab(&path),
            Err(err) => self.sandbox_status = format!("Scratchpad setup failed: {err}"),
        }
    }

    fn open_default_sandbox_task_ledger(&mut self) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_status = "Sandbox folder not found.".to_string();
            return;
        };
        match ensure_default_sandbox_task_ledger_file(&dir) {
            Ok(path) => self.open_sandbox_file_and_focus_tab(&path),
            Err(err) => self.sandbox_status = format!("Task ledger setup failed: {err}"),
        }
    }

    fn seed_default_sandbox_task_ledger_from_context(&mut self) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_status = "Sandbox folder not found.".to_string();
            return;
        };
        let current_task = self
            .chat_log
            .iter()
            .rev()
            .find(|(speaker, _)| speaker.eq_ignore_ascii_case("you"))
            .map(|(_, content)| truncate_for_ui(content.trim(), 500))
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| "Capture the current task here.".to_string());
        let next_step = self
            .memory_jogger_items()
            .last()
            .map(|item| truncate_for_ui(item.trim(), 220))
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| "Record the next concrete step here.".to_string());
        let files_touched = self
            .sandbox_editor_path
            .as_ref()
            .and_then(|path| path.strip_prefix(&dir).ok())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .into_iter()
            .collect::<Vec<_>>();
        let notes = self
            .recent_chat_exchange_pairs(Some(180), 4)
            .into_iter()
            .collect::<Vec<_>>();
        match sandbox_write_task_ledger(
            &dir,
            "active",
            &current_task,
            &next_step,
            &Vec::new(),
            &files_touched,
            &notes,
        ) {
            Ok(path) => {
                self.sandbox_status = format!("Seeded task ledger at {}", path.display());
                self.open_sandbox_file_in_editor(&path);
            }
            Err(err) => {
                self.sandbox_status = format!("Could not seed task ledger: {err}");
            }
        }
    }

    fn reopen_last_sandbox_working_file(&mut self) {
        let Some(path) = self.sandbox_last_working_path.clone() else {
            self.sandbox_status = "No sandbox working file has been opened yet.".to_string();
            return;
        };
        self.open_sandbox_file_and_focus_tab(&path);
    }

    fn current_sandbox_editor_rel_path(&self, dir: &Path) -> Option<String> {
        self.sandbox_editor_path
            .as_ref()
            .and_then(|path| path.strip_prefix(dir).ok())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
    }

    fn append_memory_jogger_note(&mut self, note: &str) {
        let safe = Self::prepare_memory_text(note, Some(280));
        if safe.trim().is_empty() {
            return;
        }
        let line = format!("- {safe}");
        self.memory_jogger = if self.memory_jogger.trim().is_empty() {
            line
        } else {
            format!("{}\n{}", self.memory_jogger.trim_end(), line)
        };
        let _ = fs::write(
            memory_jogger_path(&self.base_path),
            format!("{}\n", self.memory_jogger),
        );
    }

    fn promote_editor_text_to_scratchpad(&mut self) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_status = "Sandbox folder not found.".to_string();
            return;
        };
        let text = self.sandbox_editor_text.trim().to_string();
        if text.is_empty() {
            self.sandbox_status = "Editor is empty. Nothing to promote.".to_string();
            return;
        }
        let source = self
            .current_sandbox_editor_rel_path(&dir)
            .unwrap_or_else(|| "(unsaved scratch buffer)".to_string());
        let block = format!(
            "\n## Promoted note ({})\nSource: `{}`\n\n{}\n",
            Utc::now().to_rfc3339(),
            source,
            text
        );
        match sandbox_append(&dir, DEFAULT_SANDBOX_SCRATCHPAD_REL_PATH, &block) {
            Ok(path) => {
                self.sandbox_status = format!("Promoted editor text into {}", path.display());
                self.open_sandbox_file_in_editor(&path);
            }
            Err(err) => {
                self.sandbox_status = format!("Could not promote to scratchpad: {err}");
            }
        }
    }

    fn promote_editor_text_to_ledger_notes(&mut self) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_status = "Sandbox folder not found.".to_string();
            return;
        };
        let text = self.sandbox_editor_text.trim().to_string();
        if text.is_empty() {
            self.sandbox_status = "Editor is empty. Nothing to promote.".to_string();
            return;
        }
        self.ensure_default_sandbox_task_ledger();
        let mut summary = read_task_ledger_summary(&dir).unwrap_or_default();
        summary.notes.push(text);
        if let Some(rel_path) = self.current_sandbox_editor_rel_path(&dir) {
            if !summary.files_touched.iter().any(|item| item == &rel_path) {
                summary.files_touched.push(rel_path);
            }
        }
        match sandbox_write_task_ledger(
            &dir,
            if summary.status.trim().is_empty() {
                "active"
            } else {
                summary.status.trim()
            },
            &summary.current_task,
            &summary.next_step,
            &summary.open_questions,
            &summary.files_touched,
            &summary.notes,
        ) {
            Ok(path) => {
                self.sandbox_status = format!("Promoted editor text into {}", path.display());
                self.open_sandbox_file_in_editor(&path);
            }
            Err(err) => {
                self.sandbox_status = format!("Could not promote to ledger notes: {err}");
            }
        }
    }

    fn set_task_ledger_field_from_editor(&mut self, set_current_task: bool) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_status = "Sandbox folder not found.".to_string();
            return;
        };
        let text = Self::prepare_memory_text(&self.sandbox_editor_text, Some(500));
        if text.trim().is_empty() {
            self.sandbox_status = "Editor is empty. Nothing to promote.".to_string();
            return;
        }
        self.ensure_default_sandbox_task_ledger();
        let mut summary = read_task_ledger_summary(&dir).unwrap_or_default();
        if set_current_task {
            summary.current_task = text;
        } else {
            summary.next_step = text;
        }
        if let Some(rel_path) = self.current_sandbox_editor_rel_path(&dir) {
            if !summary.files_touched.iter().any(|item| item == &rel_path) {
                summary.files_touched.push(rel_path);
            }
        }
        summary.notes.push(format!(
            "{} set from sandbox editor on {}",
            if set_current_task {
                "Current task"
            } else {
                "Next step"
            },
            Utc::now().to_rfc3339()
        ));
        match sandbox_write_task_ledger(
            &dir,
            if summary.status.trim().is_empty() {
                "active"
            } else {
                summary.status.trim()
            },
            &summary.current_task,
            &summary.next_step,
            &summary.open_questions,
            &summary.files_touched,
            &summary.notes,
        ) {
            Ok(path) => {
                self.sandbox_status = format!("Updated {}", path.display());
                self.open_sandbox_file_in_editor(&path);
            }
            Err(err) => {
                self.sandbox_status = format!("Could not update task ledger: {err}");
            }
        }
    }

    fn append_editor_summary_to_memory_jogger(&mut self) {
        let text = Self::prepare_memory_text(&self.sandbox_editor_text, Some(240));
        if text.trim().is_empty() {
            self.sandbox_status = "Editor is empty. Nothing to summarize.".to_string();
            return;
        }
        let source = self
            .sandbox_editor_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("scratch");
        let note = format!("Sandbox note ({source}): {text}");
        self.append_memory_jogger_note(&note);
        if let Some(bookkeeper) = &self.bookkeeper {
            bookkeeper.append_event("sandbox", "Sandbox", &note, None);
        }
        self.sandbox_status = "Appended editor summary to memory jogger.".to_string();
    }

    fn defer_pending_sandbox_actions(&mut self) {
        let deferred_count = self.pending_sandbox_actions.len();
        self.pending_sandbox_actions.clear();
        self.sandbox_action_status = if deferred_count == 0 {
            "No sandbox actions were waiting to be deferred.".to_string()
        } else {
            format!("Deferred {deferred_count} sandbox action(s). No file changes were run.")
        };
        if let Some(bookkeeper) = &self.bookkeeper {
            bookkeeper.append_event(
                "sandbox",
                "Sandbox",
                &self.sandbox_action_status,
                Some("Deferred pending sandbox actions".to_string()),
            );
        }
    }

    fn preload_sandbox_and_continue(&mut self) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_action_status = "Sandbox folder not found.".to_string();
            self.pending_sandbox_actions.clear();
            return;
        };

        self.ensure_default_sandbox_scratchpad();
        self.ensure_default_sandbox_task_ledger();

        let mut paths = Vec::new();
        for action in &self.pending_sandbox_actions {
            match action {
                SandboxAction::Write { path, .. }
                | SandboxAction::Append { path, .. }
                | SandboxAction::Read { path } => {
                    if !path.trim().is_empty() {
                        paths.push(path.trim().to_string());
                    }
                }
                SandboxAction::Preload { paths: extra, .. } => {
                    for path in extra {
                        if !path.trim().is_empty() {
                            paths.push(path.trim().to_string());
                        }
                    }
                }
                SandboxAction::Ledger { files_touched, .. } => {
                    for path in files_touched {
                        if !path.trim().is_empty() {
                            paths.push(path.trim().to_string());
                        }
                    }
                }
                SandboxAction::List => {}
            }
        }
        if let Some(editor_rel_path) = self.current_sandbox_editor_rel_path(&dir) {
            paths.push(editor_rel_path);
        }
        paths.sort();
        paths.dedup();

        match sandbox_preload(
            &dir,
            &paths,
            true,
            true,
            true,
            "fast preload before continuing a multi-step task",
        ) {
            Ok(result) => {
                self.pending_sandbox_actions.clear();
                self.sandbox_last_tool_result = result.prompt_block;
                self.sandbox_action_status = format!(
                    "Preloaded {} item(s); pending sandbox actions were deferred.",
                    result.loaded_count
                );
                self.open_default_sandbox_scratchpad();
                self.continue_chat_after_sandbox(
                    "Continue from the sandbox preload context and help with the current task. Reconsider the deferred sandbox actions, and only request new sandbox JSON if it is still needed.",
                );
            }
            Err(err) => {
                self.sandbox_action_status = format!("Sandbox preload failed: {err}");
            }
        }
    }

    fn apply_pending_sandbox_actions(&mut self, continue_after: bool) {
        let Some(dir) = self.sandbox_dir.clone() else {
            self.sandbox_action_status = "Sandbox folder not found.".to_string();
            self.pending_sandbox_actions.clear();
            return;
        };

        let mut status_lines = Vec::new();
        let mut result_lines = Vec::new();
        let mut last_opened: Option<PathBuf> = None;

        for action in self.pending_sandbox_actions.drain(..) {
            match action {
                SandboxAction::Write { path, contents } => {
                    match sandbox_write(&dir, &path, &contents) {
                        Ok(path_buf) => {
                            status_lines.push(format!("Wrote {}", path_buf.display()));
                            result_lines.push(format!("sandbox.write `{path}` succeeded."));
                            last_opened = Some(path_buf);
                        }
                        Err(err) => {
                            status_lines.push(format!("Write blocked/failed ({path}): {err}"))
                        }
                    }
                }
                SandboxAction::Append { path, contents } => {
                    match sandbox_append(&dir, &path, &contents) {
                        Ok(path_buf) => {
                            status_lines.push(format!("Appended {}", path_buf.display()));
                            result_lines.push(format!("sandbox.append `{path}` succeeded."));
                            last_opened = Some(path_buf);
                        }
                        Err(err) => {
                            status_lines.push(format!("Append blocked/failed ({path}): {err}"))
                        }
                    }
                }
                SandboxAction::Read { path } => match sandbox_read(&dir, &path, 200_000) {
                    Ok(text) => {
                        result_lines.push(format!(
                            "sandbox.read `{path}` succeeded.\n{}",
                            truncate_for_ui(&text, 4_000)
                        ));
                        if let Ok(path_buf) = ensure_sandbox_save_path_within_dir(
                            &dir,
                            &dir.join(PathBuf::from(&path)),
                        ) {
                            last_opened = Some(path_buf);
                        }
                    }
                    Err(err) => status_lines.push(format!("Read blocked/failed ({path}): {err}")),
                },
                SandboxAction::List => match sandbox_list(&dir) {
                    Ok(items) => {
                        status_lines.push(format!("Sandbox files: {}", items.join(", ")));
                        result_lines.push(if items.is_empty() {
                            "sandbox.list succeeded.\n(sandbox is empty)".to_string()
                        } else {
                            format!("sandbox.list succeeded.\n{}", items.join("\n"))
                        });
                    }
                    Err(err) => status_lines.push(format!("List failed: {err}")),
                },
                SandboxAction::Ledger {
                    status,
                    current_task,
                    next_step,
                    open_questions,
                    files_touched,
                    notes,
                } => match sandbox_write_task_ledger(
                    &dir,
                    &status,
                    &current_task,
                    &next_step,
                    &open_questions,
                    &files_touched,
                    &notes,
                ) {
                    Ok(path_buf) => {
                        status_lines.push(format!("Updated {}", path_buf.display()));
                        result_lines.push(format!(
                            "sandbox.ledger updated `{}`.",
                            DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH
                        ));
                        last_opened = Some(path_buf);
                    }
                    Err(err) => status_lines.push(format!("Ledger update failed: {err}")),
                },
                SandboxAction::Preload {
                    paths,
                    include_list,
                    include_scratchpad,
                    include_ledger,
                    note,
                } => match sandbox_preload(
                    &dir,
                    &paths,
                    include_list,
                    include_scratchpad,
                    include_ledger,
                    &note,
                ) {
                    Ok(result) => {
                        status_lines.push(format!("Preloaded {} item(s)", result.loaded_count));
                        result_lines.push(result.prompt_block);
                        if include_scratchpad {
                            if let Ok(path_buf) = ensure_default_sandbox_scratchpad_file(&dir) {
                                last_opened = Some(path_buf);
                            }
                        } else if include_ledger {
                            if let Ok(path_buf) = ensure_default_sandbox_task_ledger_file(&dir) {
                                last_opened = Some(path_buf);
                            }
                        }
                    }
                    Err(err) => status_lines.push(format!("Preload failed: {err}")),
                },
            }
        }

        self.sandbox_action_status = if status_lines.is_empty() {
            "No sandbox actions were applied.".to_string()
        } else {
            status_lines.join(" | ")
        };

        if let Some(path) = last_opened {
            self.open_sandbox_file_in_editor(&path);
        }

        self.sandbox_last_tool_result = result_lines.join("\n\n");
        if let Some(bookkeeper) = &self.bookkeeper {
            let note = if self.sandbox_last_tool_result.trim().is_empty() {
                self.sandbox_action_status.clone()
            } else {
                self.sandbox_last_tool_result.clone()
            };
            bookkeeper.append_event("sandbox", "Sandbox", &note, None);
        }

        if continue_after && !self.sandbox_last_tool_result.trim().is_empty() {
            self.continue_chat_after_sandbox(
                "Continue from the approved sandbox tool result and help with the current task. If another sandbox action is needed, request it as JSON.",
            );
        }
    }

    fn build_local_presence(&self) -> LocalPresence {
        let active_tab = self
            .tabs
            .get(self.active_tab)
            .map(|tab| match &tab.kind {
                TabKind::Module { module, .. } => format!("Module: {}", module.manifest.title),
                _ => tab.title.clone(),
            })
            .unwrap_or_else(|| "Home".to_string());

        let model_label = Path::new(&self.settings.model.path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| self.settings.model.name.clone());
        let shared_room_suffix = if self.networking.snapshot().connected_peers.is_empty() {
            String::new()
        } else {
            format!(" | Room {}", self.shared_chat_policy_summary())
        };

        LocalPresence {
            active_tab,
            runtime_status: format!(
                "Role: {} | Teacher mode: {}{}",
                self.current_role(),
                self.settings.teacher_mode,
                shared_room_suffix
            ),
            model_label,
            is_generating: false,
        }
    }

    fn process_networking_changes(&mut self) {
        let received = self.networking.snapshot().received_handoffs.clone();
        for handoff in received {
            if !self
                .networking_seen_handoffs
                .insert(handoff.handoff_id.clone())
            {
                continue;
            }

            let title = if handoff.title.trim().is_empty() {
                "LAN handoff".to_string()
            } else {
                handoff.title.trim().to_string()
            };
            self.pulse_ecg(
                22.0,
                &format!("LAN handoff from {}.", handoff.from_device_name),
            );

            if let Some(bookkeeper) = &self.bookkeeper {
                bookkeeper.append_event(
                    "networking",
                    "LAN",
                    &format!(
                        "Received LAN handoff from {}: {}\n\n{}",
                        handoff.from_device_name, title, handoff.body
                    ),
                    Some(format!(
                        "From device {} at {}",
                        handoff.from_device_id, handoff.from_address
                    )),
                );
            }
        }

        let received_artifacts = self.networking.snapshot().received_artifacts.clone();
        for artifact in received_artifacts {
            if !self
                .networking_seen_artifacts
                .insert(artifact.artifact_id.clone())
            {
                continue;
            }

            if artifact.is_binary() {
                match self.store_received_generic_transfer(&artifact) {
                    Ok(path) => {
                        if let Some(bookkeeper) = &self.bookkeeper {
                            bookkeeper.append_event(
                                "networking",
                                "LAN",
                                &format!(
                                    "Received file-style transfer from {}.\n\nSaved to inbox: {}",
                                    artifact.from_device_name,
                                    path.display()
                                ),
                                Some(format!(
                                    "Kind: {} | Content type: {} | File: {}",
                                    artifact.kind, artifact.content_type, artifact.file_name
                                )),
                            );
                        }
                        self.pulse_ecg(
                            18.0,
                            &format!(
                                "File-style transfer from {} saved to inbox.",
                                artifact.from_device_name
                            ),
                        );
                    }
                    Err(err) => {
                        self.networking_status = Some(format!(
                            "Received file-style transfer from {} but could not save it: {}",
                            artifact.from_device_name, err
                        ));
                    }
                }
                continue;
            }

            match artifact.kind.as_str() {
                "homework_pack_json" => match self.store_received_homework_pack(&artifact) {
                    Ok(path) => {
                        if let Some(bookkeeper) = &self.bookkeeper {
                            bookkeeper.append_event(
                                "networking",
                                "LAN",
                                &format!(
                                    "Received homework pack from {}.\n\nLabel: {}\nSaved to: {}",
                                    artifact.from_device_name,
                                    artifact.label,
                                    path.display()
                                ),
                                Some(format!(
                                    "Artifact kind: {} | File: {}",
                                    artifact.kind, artifact.file_name
                                )),
                            );
                        }
                        self.pulse_ecg(
                            26.0,
                            &format!("Received homework pack from {}.", artifact.from_device_name),
                        );
                    }
                    Err(err) => {
                        self.networking_status = Some(format!(
                            "Received pack from {} but could not import it: {}",
                            artifact.from_device_name, err
                        ));
                    }
                },
                "workflow_bundle_json" => match self.store_received_workflow_bundle(&artifact) {
                    Ok(path) => {
                        if let Some(bookkeeper) = &self.bookkeeper {
                            bookkeeper.append_event(
                                "networking",
                                "LAN",
                                &format!(
                                    "Received classroom setup bundle from {}.\n\nLabel: {}\nSaved to inbox: {}",
                                    artifact.from_device_name,
                                    if artifact.label.trim().is_empty() {
                                        "(untitled bundle)"
                                    } else {
                                        artifact.label.trim()
                                    },
                                    path.display()
                                ),
                                Some(format!(
                                    "Artifact kind: {} | File: {}",
                                    artifact.kind, artifact.file_name
                                )),
                            );
                        }
                        self.pulse_ecg(
                            20.0,
                            &format!(
                                "Received classroom setup bundle from {}.",
                                artifact.from_device_name
                            ),
                        );
                    }
                    Err(err) => {
                        self.networking_status = Some(format!(
                            "Received setup bundle from {} but could not save it: {}",
                            artifact.from_device_name, err
                        ));
                    }
                },
                "revision_pack_markdown" => match self.store_received_revision_pack(&artifact) {
                    Ok(path) => {
                        if let Some(bookkeeper) = &self.bookkeeper {
                            bookkeeper.append_event(
                                "networking",
                                "LAN",
                                &format!(
                                    "Received revision pack from {}.\n\nLabel: {}\nSaved to inbox: {}",
                                    artifact.from_device_name,
                                    artifact.label,
                                    path.display()
                                ),
                                Some(format!(
                                    "Artifact kind: {} | File: {}",
                                    artifact.kind, artifact.file_name
                                )),
                            );
                        }
                        self.pulse_ecg(
                            24.0,
                            &format!("Received revision pack from {}.", artifact.from_device_name),
                        );
                    }
                    Err(err) => {
                        self.networking_status = Some(format!(
                            "Received revision pack from {} but could not save it: {}",
                            artifact.from_device_name, err
                        ));
                    }
                },
                "module_shared_state_json" => {
                    match self.store_received_module_shared_state(&artifact) {
                        Ok(path) => {
                            if let Some(bookkeeper) = &self.bookkeeper {
                                bookkeeper.append_event(
                                    "networking",
                                    "LAN",
                                    &format!(
                                        "Received shared module state for `{}` from {}.\n\nSaved to {}",
                                        artifact.module_id,
                                        artifact.from_device_name,
                                        path.display()
                                    ),
                                    Some(format!(
                                        "Module summary: {}",
                                        artifact.summary.trim()
                                    )),
                                );
                            }
                            self.pulse_ecg(
                                18.0,
                                &format!(
                                    "Received shared module state for {}.",
                                    artifact.module_id
                                ),
                            );
                        }
                        Err(err) => {
                            self.networking_status = Some(format!(
                                "Received shared module state for `{}` but could not save it: {}",
                                artifact.module_id, err
                            ));
                        }
                    }
                }
                "module_shared_state_ack_json" => {
                    match self.store_received_module_session_ack(&artifact) {
                        Ok(ack) => {
                            let from_label = if ack.from_device_name.trim().is_empty() {
                                ack.from_device_id.clone()
                            } else {
                                ack.from_device_name.clone()
                            };
                            let result = if ack.applied {
                                "applied"
                            } else if ack.stale {
                                "marked stale"
                            } else {
                                "did not apply"
                            };
                            self.networking_status = Some(format!(
                                "{} {} session {} revision {} for {}.",
                                from_label,
                                result,
                                if ack.session_id.trim().is_empty() {
                                    "(legacy)"
                                } else {
                                    ack.session_id.trim()
                                },
                                ack.session_revision,
                                ack.module_id
                            ));
                            if let Some(bookkeeper) = &self.bookkeeper {
                                bookkeeper.append_event(
                                    "networking",
                                    "LAN",
                                    &format!(
                                        "{} {} session {} revision {} for module `{}`.",
                                        from_label,
                                        result,
                                        if ack.session_id.trim().is_empty() {
                                            "(legacy)"
                                        } else {
                                            ack.session_id.trim()
                                        },
                                        ack.session_revision,
                                        ack.module_id
                                    ),
                                    Some(ack.message.clone()),
                                );
                            }
                            self.pulse_ecg(
                                18.0,
                                &format!("{} {} {}.", from_label, result, ack.module_id),
                            );
                        }
                        Err(err) => {
                            self.networking_status = Some(format!(
                                "Received a module session receipt from {} but could not read it: {}",
                                artifact.from_device_name, err
                            ));
                        }
                    }
                }
                "shared_chat_policy_json" => {
                    if let Err(err) = self.apply_received_shared_chat_policy(&artifact) {
                        self.networking_status = Some(format!(
                            "Received a shared room policy from {} but could not read it: {}",
                            artifact.from_device_name, err
                        ));
                    }
                }
                "shared_chat_message_json" => {
                    if let Err(err) = self.apply_received_shared_chat_message(&artifact) {
                        self.networking_status = Some(format!(
                            "Received a shared room message from {} but could not read it: {}",
                            artifact.from_device_name, err
                        ));
                    }
                }
                "lukewarm_context_json" => match self.store_received_lukewarm_context(&artifact) {
                    Ok(path) => {
                        let summary = if artifact.summary.trim().is_empty() {
                            "Shared luke warm context received.".to_string()
                        } else {
                            artifact.summary.trim().to_string()
                        };
                        self.networking_status = Some(format!(
                            "Received shared luke warm context from {} and saved it to the inbox.",
                            artifact.from_device_name
                        ));
                        if let Some(bookkeeper) = &self.bookkeeper {
                            bookkeeper.append_event(
                                "networking",
                                "LAN",
                                &format!(
                                    "Received shared luke warm context from {}.",
                                    artifact.from_device_name
                                ),
                                Some(format!("Saved to inbox: {}\n{}", path.display(), summary)),
                            );
                        }
                        self.pulse_ecg(
                            18.0,
                            &format!(
                                "Luke warm context from {} saved to inbox.",
                                artifact.from_device_name
                            ),
                        );
                    }
                    Err(err) => {
                        self.networking_status = Some(format!(
                            "Received shared luke warm context from {} but could not save it: {}",
                            artifact.from_device_name, err
                        ));
                    }
                },
                _ => match self.store_received_generic_transfer(&artifact) {
                    Ok(path) => {
                        if let Some(bookkeeper) = &self.bookkeeper {
                            bookkeeper.append_event(
                                "networking",
                                "LAN",
                                &format!(
                                    "Received transfer `{}` from {}.\n\nSaved to inbox: {}",
                                    artifact.kind,
                                    artifact.from_device_name,
                                    path.display()
                                ),
                                Some(format!(
                                    "Summary: {}",
                                    if artifact.summary.trim().is_empty() {
                                        "(no summary)"
                                    } else {
                                        artifact.summary.trim()
                                    }
                                )),
                            );
                        }
                        self.pulse_ecg(
                            16.0,
                            &format!(
                                "Transfer from {} saved to inbox.",
                                artifact.from_device_name
                            ),
                        );
                    }
                    Err(err) => {
                        self.networking_status = Some(format!(
                            "Received transfer `{}` from {} but could not save it: {}",
                            artifact.kind, artifact.from_device_name, err
                        ));
                    }
                },
            }
        }
    }

    fn open_module_tab(&mut self, module: &LoadedModule) {
        if module.manifest.id == "homework_dashboard" && !self.teacher_unlocked {
            self.teacher_pin_status =
                Some("Unlock teacher view to open the homework dashboard.".to_string());
            return;
        }
        self.pulse_ecg(12.0, &format!("Opened {}.", module.manifest.title));
        let key = format!("module:{}", module.manifest.id);
        let m = module.clone();
        let tab_key = key.clone();
        self.open_or_focus_tab(&key, |_app| Tab {
            id: 0,
            title: m.manifest.title.clone(),
            kind: TabKind::Module {
                module: m,
                cached_text: None,
            },
            closable: true,
            key: tab_key,
        });
    }

    fn is_module_tab_open(&self, module_id: &str) -> bool {
        self.tabs.iter().any(|tab| {
            matches!(
                &tab.kind,
                TabKind::Module { module, .. } if module.manifest.id == module_id
            )
        })
    }

    fn module_kind_label(module: &LoadedModule) -> &'static str {
        if let Some(visual) = &module.manifest.visual_load {
            if visual.is_webview() {
                "Hosted web dashboard"
            } else {
                "Hosted native app"
            }
        } else {
            match module.manifest.entry.as_ref() {
                Some(ModuleEntry::BuiltinPanel { .. }) => "Built-in EDU panel",
                Some(ModuleEntry::Markdown { .. }) => "Markdown surface",
                Some(ModuleEntry::StaticHtml { .. }) => "Static HTML surface",
                Some(ModuleEntry::ExternalProcess { .. }) => "External process module",
                None => "Portable fallback module",
            }
        }
    }

    fn module_hover_text(module: &LoadedModule) -> String {
        let mut parts = vec![Self::module_kind_label(module).to_string()];
        if let Some(description) = module
            .manifest
            .description
            .as_ref()
            .map(|text| text.trim())
            .filter(|text| !text.is_empty())
        {
            parts.push(description.to_string());
        }
        if let Some(author) = module
            .manifest
            .author
            .as_ref()
            .map(|text| text.trim())
            .filter(|text| !text.is_empty())
        {
            parts.push(format!("Author: {author}"));
        }
        if !module.manifest.roles.is_empty() {
            parts.push(format!("Roles: {}", module.manifest.roles.join(", ")));
        }
        parts.join("\n")
    }

    fn render_module_launcher_entry(&mut self, ui: &mut egui::Ui, module: &LoadedModule) {
        let label = if self.is_module_tab_open(&module.manifest.id) {
            format!("{} (open)", module.manifest.title)
        } else {
            module.manifest.title.clone()
        };
        let response = ui
            .button(label)
            .on_hover_text(Self::module_hover_text(module));
        if response.clicked() {
            self.open_module_tab(module);
            ui.close_menu();
        }
    }

    fn render_module_group_menu(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        modules: &[LoadedModule],
    ) {
        if modules.is_empty() {
            return;
        }
        ui.menu_button(title, |ui| {
            for module in modules {
                self.render_module_launcher_entry(ui, module);
            }
        });
    }

    fn render_modules_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button("Reload modules").clicked() {
            self.reload_modules();
            ui.close_menu();
            return;
        }

        ui.separator();
        ui.label(format!("Current role: {}", self.current_role()));

        if self.modules.is_empty() {
            ui.label("No modules found.");
            return;
        }

        let current_role = self.current_role().to_owned();
        let modules = self.modules.clone();
        let mut hosted_web = Vec::new();
        let mut hosted_native = Vec::new();
        let mut builtin_panels = Vec::new();
        let mut fallback = Vec::new();
        let mut hidden_for_role = 0usize;

        for module in modules {
            if module.manifest.id == "homework_dashboard" && !self.teacher_unlocked {
                hidden_for_role += 1;
                continue;
            }
            if !role_allowed(&module.manifest, current_role.as_str()) {
                hidden_for_role += 1;
                continue;
            }

            if let Some(visual) = &module.manifest.visual_load {
                if visual.is_webview() {
                    hosted_web.push(module);
                } else {
                    hosted_native.push(module);
                }
            } else if matches!(
                module.manifest.entry.as_ref(),
                Some(ModuleEntry::BuiltinPanel { .. })
            ) {
                builtin_panels.push(module);
            } else {
                fallback.push(module);
            }
        }

        if hidden_for_role > 0 && !self.teacher_unlocked {
            ui.label("Unlock teacher view to see teacher-only modules.");
        }

        if hosted_web.is_empty()
            && hosted_native.is_empty()
            && builtin_panels.is_empty()
            && fallback.is_empty()
        {
            ui.label("No modules available for the current role.");
            return;
        }

        ui.menu_button("Hosted UIs", |ui| {
            self.render_module_group_menu(ui, "Web dashboards", &hosted_web);
            self.render_module_group_menu(ui, "Native apps", &hosted_native);
            if hosted_web.is_empty() && hosted_native.is_empty() {
                ui.label("No hosted modules available.");
            }
        });

        ui.menu_button("Built-in / Fallback", |ui| {
            self.render_module_group_menu(ui, "Built-in EDU panels", &builtin_panels);
            self.render_module_group_menu(ui, "Reference and fallback surfaces", &fallback);
            if builtin_panels.is_empty() && fallback.is_empty() {
                ui.label("No built-in or fallback modules available.");
            }
        });
    }

    fn request_close_module(&mut self, module_id: &str) {
        let visual = self
            .modules
            .iter()
            .find(|module| module.manifest.id == module_id)
            .and_then(|module| module.manifest.visual_load.clone());

        if let Some(visual) = visual {
            if let Some(host) = self.module_hosts.get_mut(module_id) {
                if host.is_running() {
                    host.request_close(&visual);
                    self.close_pending_modules.insert(module_id.to_string());
                    return;
                }
            }
        }

        self.close_module_tab_by_id(module_id);
    }

    fn close_module_tab_by_id(&mut self, module_id: &str) {
        self.close_pending_modules.remove(module_id);

        if let Some(mut host) = self.module_hosts.remove(module_id) {
            host.force_stop();
        }
        self.module_host_targets.remove(module_id);

        if let Some(idx) = self.tabs.iter().position(|tab| {
            matches!(
                &tab.kind,
                TabKind::Module { module, .. } if module.manifest.id == module_id
            )
        }) {
            self.tabs.remove(idx);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len().saturating_sub(1);
            } else if idx <= self.active_tab && self.active_tab > 0 {
                self.active_tab -= 1;
            }
        }
    }

    fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() || !self.tabs[idx].closable {
            return;
        }

        if let TabKind::Module { module, .. } = &self.tabs[idx].kind {
            let module_id = module.manifest.id.clone();
            self.request_close_module(&module_id);
            return;
        }

        self.tabs.remove(idx);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
    }

    fn set_module_host_target(&mut self, module_id: &str, rect: egui::Rect, pixels_per_point: f32) {
        let scale = pixels_per_point.max(1.0);
        self.module_host_targets.insert(
            module_id.to_string(),
            HostRect {
                x: (rect.min.x * scale).round() as i32,
                y: (rect.min.y * scale).round() as i32,
                width: (rect.width() * scale).round() as i32,
                height: (rect.height() * scale).round() as i32,
            },
        );
    }

    fn sync_module_hosts(&mut self) -> bool {
        if self.module_hosts.is_empty() {
            return false;
        }

        let mut needs_repaint = false;
        let mut close_ready = Vec::new();
        let host_ids = self.module_hosts.keys().cloned().collect::<Vec<_>>();

        for module_id in host_ids {
            let module_meta = self
                .modules
                .iter()
                .find(|module| module.manifest.id == module_id)
                .and_then(|module| {
                    module
                        .manifest
                        .visual_load
                        .clone()
                        .map(|visual| (module.folder.clone(), visual))
                });

            let Some((module_dir, visual)) = module_meta else {
                if let Some(host) = self.module_hosts.get_mut(&module_id) {
                    host.force_stop();
                }
                close_ready.push(module_id);
                continue;
            };

            let target = self.module_host_targets.get(&module_id).copied();
            if let Some(host) = self.module_hosts.get_mut(&module_id) {
                if host.sync(&module_dir, &visual, target) {
                    needs_repaint = true;
                }
                if self.close_pending_modules.contains(&module_id) && host.ready_to_finish_close() {
                    close_ready.push(module_id.clone());
                }
            }
        }

        for module_id in close_ready {
            self.close_module_tab_by_id(&module_id);
        }

        needs_repaint
    }

    fn read_module_bridge_status(
        &self,
        module_id: &str,
        module_dir: &Path,
    ) -> Option<crate::module_bridge::ModuleBridgeStatus> {
        match read_bridge_status(module_dir) {
            Ok(Some(status))
                if status.module_id.trim().is_empty() || status.module_id.trim() == module_id =>
            {
                Some(status)
            }
            Ok(_) => None,
            Err(_) => None,
        }
    }

    fn read_module_bridge_shared_state(
        &self,
        module_id: &str,
        module_dir: &Path,
    ) -> Option<ModuleBridgeSharedState> {
        match read_bridge_shared_state(module_dir) {
            Ok(Some(state))
                if state.module_id.trim().is_empty() || state.module_id.trim() == module_id =>
            {
                Some(state)
            }
            Ok(_) => None,
            Err(_) => None,
        }
    }

    fn read_module_bridge_incoming_shared_state(
        &self,
        module_id: &str,
        module_dir: &Path,
    ) -> Option<ModuleBridgeIncomingSharedState> {
        match read_bridge_incoming_shared_state(module_dir) {
            Ok(Some(state))
                if state.module_id.trim().is_empty() || state.module_id.trim() == module_id =>
            {
                Some(state)
            }
            Ok(_) => None,
            Err(_) => None,
        }
    }

    fn read_module_bridge_incoming_assets(
        &mut self,
        module_id: &str,
        module_dir: &Path,
        lane_id: Option<&str>,
    ) -> Vec<ModuleBridgeIncomingAssetRecord> {
        match read_bridge_incoming_assets(module_dir, lane_id) {
            Ok(records) => records
                .into_iter()
                .filter(|record| {
                    record.module_id.trim().is_empty() || record.module_id.trim() == module_id
                })
                .collect(),
            Err(err) => {
                self.networking_status = Some(format!(
                    "Runtime: incoming asset read warning for {module_id}: {err:#}"
                ));
                Vec::new()
            }
        }
    }

    fn read_module_bridge_log_context(&self, module_dir: &Path) -> Option<String> {
        let excerpts = read_bridge_log_excerpts(module_dir).ok()?;
        if excerpts.is_empty() {
            return None;
        }

        let mut blocks = Vec::new();
        for excerpt in excerpts {
            if excerpt.excerpt.trim().is_empty() {
                continue;
            }
            blocks.push(format!(
                "## {}\n{}\n{}",
                excerpt.label.trim(),
                excerpt.path.trim(),
                excerpt.excerpt.trim()
            ));
        }

        if blocks.is_empty() {
            None
        } else {
            Some(blocks.join("\n\n"))
        }
    }

    fn render_menu_bar(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Reload modules").clicked() {
                    self.reload_modules();
                    ui.close_menu();
                }
                ui.menu_button("Files", |ui| {
                    ui.label(format!("Base path: {}", self.base_path.display()));
                    if ui.button("Open Diagnostic / Health Check").clicked() {
                        self.open_diagnostics_tab();
                        ui.close_menu();
                    }
                    if ui.button("Copy Diagnostic report").clicked() {
                        let audience = if self.teacher_unlocked {
                            DiagnosticsAudience::Teacher
                        } else {
                            DiagnosticsAudience::StudentSafe
                        };
                        let report = self.build_diagnostics_report(audience);
                        ctx.copy_text(report.clone());
                        self.diagnostics_audience = audience;
                        self.diagnostics_report = report;
                        self.diagnostics_status =
                            Some("Copied diagnostic report to clipboard.".to_string());
                        ui.close_menu();
                    }
                    if let Some(msg) = &self.diagnostics_status {
                        ui.separator();
                        ui.label(msg);
                    }
                });
                ui.menu_button("Models", |ui| {
                    let models = self.available_models.clone();
                    let current_path = self.settings.model.path.clone();
                    let main_role_label = if self.settings.model.path.trim().is_empty() {
                        "No model selected".to_string()
                    } else {
                        self.settings.model.name.clone()
                    };
                    if models.is_empty() {
                        ui.label("No GGUF models found yet.");
                        ui.label("Drop a GGUF into data/models/ to get started.");
                    } else {
                        ui.label(format!("Main AI role: {main_role_label}"));
                        if self.teacher_unlocked {
                            ui.label(format!(
                                "Bookkeeper role: {}",
                                if self.settings.bookkeeper_model_name.trim().is_empty() {
                                    "Keyword-only background summary mode"
                                } else {
                                    &self.settings.bookkeeper_model_name
                                }
                            ));
                        }
                        ui.separator();
                    }
                    for model in models {
                        let selected = Path::new(&current_path) == model.path;
                        let file_label = model
                            .path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| model.path.to_string_lossy().to_string());
                        let label = format!("{} ({})", model.name, file_label);
                        if ui.selectable_label(selected, label).clicked() {
                            self.select_model(&model);
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button("Refresh models list").clicked() {
                        self.reload_models();
                        ui.close_menu();
                    }
                    if self.teacher_unlocked {
                        ui.separator();
                        if ui.button("Open bookkeeper logs").clicked() {
                            self.open_bookkeeper_tab();
                            ui.close_menu();
                        }
                    }
                    ui.label(format!(
                        "Folder: {}",
                        self.base_path.join("models").display()
                    ));
                });
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.menu_button("View", |ui| {
                let preset_names: Vec<String> =
                    self.presets.iter().map(|p| p.name.clone()).collect();
                for name in preset_names {
                    let selected = self.theme.name == name;
                    if ui.selectable_label(selected, name.clone()).clicked() {
                        self.switch_theme(&name, ctx);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Open networking tab").clicked() {
                    self.open_networking_tab();
                    ui.close_menu();
                }
            });

            ui.menu_button("Modules", |ui| {
                self.render_modules_menu(ui);
            });

            ui.menu_button("Tools", |ui| {
                if ui.button("Open Sandbox").clicked() {
                    self.open_sandbox_tab();
                    ui.close_menu();
                }
                if ui.button("Open scratchpad").clicked() {
                    self.open_default_sandbox_scratchpad();
                    ui.close_menu();
                }
                if ui.button("Open task ledger").clicked() {
                    self.open_default_sandbox_task_ledger();
                    ui.close_menu();
                }
            });

            ui.menu_button("Network", |ui| {
                if ui.button("Open networking tab").clicked() {
                    self.open_networking_tab();
                    ui.close_menu();
                }

                let snapshot = self.networking.snapshot().clone();
                let mut available = snapshot.available_for_connectivity;
                if ui
                    .checkbox(&mut available, "Make available for connectivity")
                    .changed()
                {
                    self.networking.set_available(available);
                }
                if ui.button("Refresh discovery").clicked() {
                    self.networking.refresh_discovery();
                }
                ui.separator();
                ui.label(format!("This device: {}", snapshot.device_name));
                ui.small(format!(
                    "Available peers: {}",
                    snapshot.discovered_peers.len()
                ));
                ui.small(format!(
                    "Connected peers: {}",
                    snapshot.connected_peers.len()
                ));
            });

            ui.menu_button("Teacher", |ui| {
                ui.label(format!(
                    "Status: {}",
                    if self.teacher_unlocked {
                        "Unlocked"
                    } else {
                        "Locked"
                    }
                ));
                ui.label(format!("Role: {}", self.current_role()));
                ui.separator();
                if !self.teacher_unlocked {
                    ui.label("Enter PIN to unlock teacher view");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.teacher_pin_input)
                            .password(true)
                            .hint_text("0000"),
                    );
                    if ui.button("Unlock").clicked() {
                        self.try_unlock_teacher();
                        ui.close_menu();
                    }
                    if !self.settings.teacher_secret_question.is_empty() {
                        ui.separator();
                        ui.label("Forgot PIN? Answer secret question:");
                        ui.label(format!("Q: {}", self.settings.teacher_secret_question));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.teacher_secret_answer_input)
                                .password(true)
                                .hint_text("Answer"),
                        );
                        if ui.button("Unlock with answer").clicked() {
                            if self.settings.teacher_secret_answer
                                == self.teacher_secret_answer_input.trim()
                            {
                                self.teacher_unlocked = true;
                                self.teacher_pin_status =
                                    Some("Unlocked via secret question".to_string());
                            } else {
                                self.teacher_pin_status =
                                    Some("Incorrect secret answer".to_string());
                            }
                            self.teacher_secret_answer_input.clear();
                            ui.close_menu();
                        }
                    }
                } else {
                    if ui.button("Open teacher dashboard").clicked() {
                        self.open_teacher_dashboard();
                        ui.close_menu();
                    }
                    if ui.button("Rescan packs + submissions").clicked() {
                        self.resync_homework();
                        self.resync_revision();
                        self.teacher_pin_status =
                            Some("Rescanned packs and submissions.".to_string());
                    }
                    ui.separator();
                    ui.label("Class mode");
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.settings.teacher_mode != "class",
                                egui::Button::new("Set CLASS"),
                            )
                            .clicked()
                        {
                            self.settings.teacher_mode = "class".to_string();
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status =
                                Some("Teacher mode set to CLASS.".to_string());
                        }
                        if ui
                            .add_enabled(
                                self.settings.teacher_mode != "free_time",
                                egui::Button::new("Set FREE TIME"),
                            )
                            .clicked()
                        {
                            self.settings.teacher_mode = "free_time".to_string();
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status =
                                Some("Teacher mode set to FREE TIME.".to_string());
                        }
                        ui.label(format!("Current: {}", self.settings.teacher_mode));
                    });
                    ui.separator();
                    ui.label("Games");
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!self.settings.game.enabled, egui::Button::new("Games ON"))
                            .clicked()
                        {
                            self.settings.game.enabled = true;
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status = Some("Games enabled.".to_string());
                        }
                        if ui
                            .add_enabled(self.settings.game.enabled, egui::Button::new("Games OFF"))
                            .clicked()
                        {
                            self.settings.game.enabled = false;
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status = Some("Games disabled.".to_string());
                        }
                        ui.label(format!(
                            "Allowed in class: {}",
                            self.settings.game.games_in_class_allowed
                        ));
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.settings.game.games_in_class_allowed,
                                egui::Button::new("Allow in class"),
                            )
                            .clicked()
                        {
                            self.settings.game.games_in_class_allowed = true;
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status =
                                Some("Games allowed in class.".to_string());
                        }
                        if ui
                            .add_enabled(
                                self.settings.game.games_in_class_allowed,
                                egui::Button::new("Forbid in class"),
                            )
                            .clicked()
                        {
                            self.settings.game.games_in_class_allowed = false;
                            let _ = save_settings(&self.settings, &self.base_path);
                            self.teacher_pin_status =
                                Some("Games forbidden in class.".to_string());
                        }
                    });
                    ui.separator();
                    if ui.button("Export pack template").clicked() {
                        match export_pack_template(
                            &self.base_path,
                            "school",
                            &self.settings.student.class_id,
                        ) {
                            Ok(path) => {
                                self.teacher_pin_status =
                                    Some(format!("Template written to {}", path.display()));
                            }
                            Err(e) => {
                                self.teacher_pin_status =
                                    Some(format!("Failed to export template: {e}"));
                            }
                        }
                    }
                    if ui.button("Import pack file...").clicked() {
                        if let Some(file) = FileDialog::new()
                            .add_filter("homework pack", &["json", "md"])
                            .pick_file()
                        {
                            let ext = file
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_ascii_lowercase();

                            if ext == "md" {
                                let outgoing_dir = homework_markdown::homework_outgoing_dir(&self.base_path);
                                let _ = fs::create_dir_all(&outgoing_dir);
                                let file_name = file
                                    .file_name()
                                    .unwrap_or_else(|| std::ffi::OsStr::new("homework_pack_import.md"));
                                let mut dest_md = outgoing_dir.join(file_name);
                                let mut n = 1usize;
                                while dest_md.exists() {
                                    dest_md = outgoing_dir.join(format!(
                                        "{}_{n}.md",
                                        dest_md
                                            .file_stem()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("homework_pack_import")
                                    ));
                                    n += 1;
                                }
                                if let Err(e) = fs::copy(&file, &dest_md) {
                                    self.teacher_pin_status = Some(format!("Import failed: {e}"));
                                } else {
                                    let defaults = PackMdDefaults {
                                        version: "1.0".to_string(),
                                        school_id: self
                                            .current_pack
                                            .as_ref()
                                            .map(|p| p.school_id.clone())
                                            .unwrap_or_else(|| "school".to_string()),
                                        class_id: if self.settings.student.class_id.trim().is_empty() {
                                            self.current_pack
                                                .as_ref()
                                                .map(|p| p.class_id.clone())
                                                .unwrap_or_else(|| "class".to_string())
                                        } else {
                                            self.settings.student.class_id.trim().to_string()
                                        },
                                    };
                                    match homework_markdown::transcribe_outgoing_packs(&self.base_path, &defaults) {
                                        Ok(report) => {
                                            self.resync_homework();
                                            if let Some(pack) = self.current_pack.clone() {
                                                apply_pack_policy(&mut self.settings, &pack);
                                                let _ = save_settings(&self.settings, &self.base_path);
                                            }
                                            self.teacher_pin_status = Some(format!(
                                                "Imported .md to outgoing and transcribed: processed {}, wrote {}, skipped {}, failed {}",
                                                report.processed, report.written, report.skipped, report.failed
                                            ));
                                        }
                                        Err(e) => {
                                            self.teacher_pin_status =
                                                Some(format!("Transcribe failed: {e}"));
                                        }
                                    }
                                }
                            } else {
                                let dest_dir = self.base_path.join("homework").join("assigned");
                                let _ = fs::create_dir_all(&dest_dir);
                                let dest = dest_dir.join(
                                    file.file_name()
                                        .unwrap_or_else(|| std::ffi::OsStr::new("homework_pack_import.json")),
                                );
                                match fs::copy(&file, &dest) {
                                    Ok(_) => match load_pack_from_file(&dest) {
                                        Ok(pack) => {
                                            apply_pack_policy(&mut self.settings, &pack);
                                            let _ = save_settings(&self.settings, &self.base_path);
                                            self.current_pack = Some(pack);
                                            self.resync_homework();
                                            self.teacher_pin_status =
                                                Some(format!("Imported {}", dest.display()));
                                        }
                                        Err(e) => {
                                            self.teacher_pin_status =
                                                Some(format!("Copied but failed to parse pack: {e}"));
                                        }
                                    },
                                    Err(e) => {
                                        self.teacher_pin_status =
                                            Some(format!("Import failed: {e}"));
                                    }
                                }
                            }
                        }
                    }
                    if ui.button("Show completed summary").clicked() {
                        let rows = self.submission_rows();
                        if rows.is_empty() {
                            self.teacher_pin_status =
                                Some("No completed submissions found.".to_string());
                        } else {
                            egui::Window::new("Completed submissions")
                                .id(egui::Id::new("teacher_completed_submissions_window"))
                                .collapsible(true)
                                .resizable(true)
                                .show(ui.ctx(), |ui| {
                                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                        for row in &rows {
                                            ui.push_id(
                                                (
                                                    "teacher_completed_summary_row",
                                                    &row.assignment_id,
                                                    &row.student_id,
                                                    &row.submitted_at,
                                                ),
                                                |ui| {
                                                    let label = format!(
                                                        "{} ({}) - {} ({}) - subj: {} - score: {} - {} - submitted: {}",
                                                        row.assignment_title,
                                                        row.assignment_id,
                                                        row.student_name,
                                                        row.student_id,
                                                        row.subject,
                                                        row.score,
                                                        row.feedback,
                                                        row.submitted_at
                                                    );
                                                    ui.label(&label);
                                                    if let Some(ai_fb) = self
                                                        .submissions
                                                        .iter()
                                                        .find(|s| {
                                                            s.assignment_id == row.assignment_id
                                                                && s.student_id
                                                                    == row.student_id
                                                        })
                                                        .and_then(|s| s.ai_feedback.clone())
                                                    {
                                                        ui.label(format!(
                                                            "AI feedback: {}",
                                                            ai_fb
                                                        ));
                                                    }
                                                },
                                            );
                                        }
                                    });
                                });
                            self.teacher_pin_status =
                                Some(format!("Completed submissions: {}", rows.len()));
                        }
                    }
                    if ui.button("Lock teacher view").clicked() {
                        self.lock_teacher();
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label("Change teacher PIN");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.teacher_pin_new)
                            .password(true)
                            .hint_text("New PIN"),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.teacher_pin_confirm)
                            .password(true)
                            .hint_text("Confirm PIN"),
                    );
                    if ui.button("Update PIN").clicked() {
                        self.change_teacher_pin();
                    }
                    ui.separator();
                    ui.label("Secret question (for PIN recovery)");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.teacher_secret_question_input)
                            .hint_text("Secret question"),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.teacher_secret_answer_input)
                            .password(true)
                            .hint_text("Answer"),
                    );
                    if ui.button("Save secret").clicked() {
                        self.update_secret_question();
                    }
                }
                if let Some(msg) = &self.teacher_pin_status {
                    ui.colored_label(self.warning_color(), msg);
                }
            });

            ui.menu_button("Settings", |ui| {
                if ui.button("Open settings tab").clicked() {
                    self.open_or_focus_tab("settings", |_app| Tab {
                        id: 0,
                        title: "Settings".to_string(),
                        kind: TabKind::Settings,
                        closable: true,
                        key: "settings".to_string(),
                    });
                    ui.close_menu();
                }
                if ui.button("Open diagnostics tab").clicked() {
                    self.open_diagnostics_tab();
                    ui.close_menu();
                }
            });

            ui.menu_button("Help", |ui| {
                ui.label(format!("Chatty-EDU v{} (egui)", env!("CARGO_PKG_VERSION")));
                ui.label(format!("Base path: {}", self.base_path.display()));
                ui.separator();
                self.render_fmi_about(ui);
            });
        });
    }

    fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        let widget_width = 176.0;
        let gap = 8.0;
        ui.horizontal(|ui| {
            let tab_width = (ui.available_width() - widget_width - gap).max(140.0);
            ui.scope(|ui| {
                ui.set_min_width(tab_width);
                ui.set_max_width(tab_width);
                ui.horizontal_wrapped(|ui| {
                    let mut to_close: Option<usize> = None;
                    for (idx, tab) in self.tabs.iter().enumerate() {
                        let active = idx == self.active_tab;
                        let module_id = match &tab.kind {
                            TabKind::Module { module, .. } => Some(module.manifest.id.clone()),
                            _ => None,
                        };
                        let close_pending = module_id
                            .as_ref()
                            .map(|module_id| self.close_pending_modules.contains(module_id))
                            .unwrap_or(false);
                        ui.horizontal(|ui| {
                            let response = if matches!(&tab.kind, TabKind::Bookkeeper) {
                                ui.selectable_label(active, tab.title.clone())
                                    .on_hover_text(
                                    "Full session logs. Search past activity and diagnose issues.",
                                )
                            } else if matches!(&tab.kind, TabKind::Networking) {
                                ui.selectable_label(active, tab.title.clone())
                                    .on_hover_text(
                                    "Local Wi-Fi / LAN links between nearby Chatty-EDU instances.",
                                )
                            } else if matches!(&tab.kind, TabKind::Sandbox) {
                                ui.selectable_label(active, tab.title.clone()).on_hover_text(
                                    "Scratchpad, task ledger, and sandbox files Chatty-EDU can use as durable working memory.",
                                )
                            } else {
                                ui.selectable_label(active, tab.title.clone())
                            };
                            if response.clicked() {
                                self.active_tab = idx;
                            }
                            if tab.closable {
                                let close_label = if close_pending { "…" } else { "x" };
                                if ui
                                    .add_enabled(!close_pending, egui::Button::new(close_label))
                                    .clicked()
                                {
                                    to_close = Some(idx);
                                }
                            }
                        });
                    }

                    if let Some(idx) = to_close {
                        self.close_tab(idx);
                    }
                });
            });
            ui.add_space(gap);
            self.render_ecg_window(ui);
        });
    }

    fn render_ecg_window(&self, ui: &mut egui::Ui) {
        let payload = self.ecg_window.payload();
        let desired_size = egui::vec2(176.0, 46.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

        let surface = color_from_hex(&self.theme.surface);
        let border = color_from_hex(&self.theme.border);
        let muted = color_from_hex(&self.theme.muted_text);
        let accent = if payload.current_percent >= 35.0 {
            color_from_hex(&self.theme.accent)
        } else {
            muted.gamma_multiply(0.9)
        };

        ui.painter().rect(
            rect,
            egui::Rounding::same(6.0),
            surface,
            egui::Stroke::new(1.0, border),
        );

        let inner = rect.shrink2(egui::vec2(8.0, 6.0));
        let small_font = egui::TextStyle::Small.resolve(ui.style());
        let body_font = egui::TextStyle::Body.resolve(ui.style());

        ui.painter().text(
            inner.left_top(),
            egui::Align2::LEFT_TOP,
            "ECG",
            small_font,
            muted,
        );
        ui.painter().text(
            inner.right_top(),
            egui::Align2::RIGHT_TOP,
            format!("{:.0}%", payload.current_percent),
            body_font,
            accent,
        );

        let chart_rect = egui::Rect::from_min_max(
            egui::pos2(inner.min.x, inner.min.y + 16.0),
            egui::pos2(inner.max.x, inner.max.y),
        );
        ui.painter().line_segment(
            [
                egui::pos2(chart_rect.left(), chart_rect.bottom()),
                egui::pos2(chart_rect.right(), chart_rect.bottom()),
            ],
            egui::Stroke::new(1.0, border.gamma_multiply(0.6)),
        );

        let points = self
            .ecg_window
            .points(chart_rect.width(), chart_rect.height())
            .into_iter()
            .map(|point| egui::pos2(chart_rect.left() + point.x, chart_rect.top() + point.y))
            .collect::<Vec<_>>();

        if points.len() >= 2 {
            ui.painter()
                .add(egui::Shape::line(points, egui::Stroke::new(1.6, accent)));
        } else if let Some(point) = points.first() {
            ui.painter().circle_filled(*point, 1.8, accent);
        }

        let state = if !payload.supported {
            "unsupported"
        } else if payload.available {
            "live"
        } else {
            "unavailable"
        };
        let _ = response.on_hover_text(format!(
            "{}\n{}\nCurrent: {:.0}%\nState: {}",
            payload.label, payload.note, payload.current_percent, state
        ));
    }

    fn render_home(&mut self, ui: &mut egui::Ui) {
        ui.heading("Home");
        ui.label(format!("Base path: {}", self.base_path.display()));
        if self.available_models.is_empty() || self.settings.model.path.trim().is_empty() {
            ui.label("AI setup: Drop a GGUF into data/models/ to get started.");
        } else {
            let current_model = Path::new(&self.settings.model.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&self.settings.model.path);
            ui.label(format!(
                "Active model: {} ({current_model})",
                self.settings.model.name
            ));
        }
        ui.label(format!("Teacher mode: {}", self.settings.teacher_mode));
        ui.label(format!("Available modules: {}", self.modules.len()));
        ui.separator();
        self.render_fmi_about(ui);
        ui.separator();
        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                ui.set_min_height(ui.available_height());
                ui.label(RichText::new("Student profile").strong());
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.settings.student.student_name);
                    ui.label("ID");
                    ui.text_edit_singleline(&mut self.settings.student.student_id);
                    ui.label("Class");
                    ui.text_edit_singleline(&mut self.settings.student.class_id);
                    if ui.button("Save profile").clicked() {
                        let _ = save_settings(&self.settings, &self.base_path);
                        self.resync_revision();
                    }
                });
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Rescan packs + submissions").clicked() {
                        self.resync_homework();
                        self.resync_revision();
                    }
                    if ui
                        .add_enabled(
                            !self.revision_sources.is_empty() || !self.past_papers.is_empty(),
                            egui::Button::new("Open Revision"),
                        )
                        .clicked()
                    {
                        self.open_revision_workspace();
                    }
                    if ui
                        .add_enabled(
                            self.teacher_unlocked,
                            egui::Button::new("Export pack template"),
                        )
                        .clicked()
                    {
                        match export_pack_template(
                            &self.base_path,
                            "school",
                            &self.settings.student.class_id,
                        ) {
                            Ok(path) => {
                                let _ = ui.label(format!("Template at {}", path.display()));
                            }
                            Err(e) => {
                                let _ = ui.label(format!("Failed: {e}"));
                            }
                        };
                    }
                    if ui
                        .add_enabled(
                            self.teacher_unlocked,
                            egui::Button::new("Import pack file..."),
                        )
                        .clicked()
                    {
                        if let Some(file) = FileDialog::new()
                            .add_filter("homework pack", &["json", "md"])
                            .pick_file()
                        {
                            let ext = file
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_ascii_lowercase();

                            if ext == "md" {
                                let outgoing_dir = homework_markdown::homework_outgoing_dir(&self.base_path);
                                let _ = fs::create_dir_all(&outgoing_dir);
                                let file_name = file
                                    .file_name()
                                    .unwrap_or_else(|| std::ffi::OsStr::new("homework_pack_import.md"));
                                let mut dest_md = outgoing_dir.join(file_name);
                                let mut n = 1usize;
                                while dest_md.exists() {
                                    dest_md = outgoing_dir.join(format!(
                                        "{}_{n}.md",
                                        dest_md
                                            .file_stem()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("homework_pack_import")
                                    ));
                                    n += 1;
                                }
                                if let Err(e) = fs::copy(&file, &dest_md) {
                                    let _ = ui.label(format!("Import failed: {e}"));
                                } else {
                                    let defaults = PackMdDefaults {
                                        version: "1.0".to_string(),
                                        school_id: self
                                            .current_pack
                                            .as_ref()
                                            .map(|p| p.school_id.clone())
                                            .unwrap_or_else(|| "school".to_string()),
                                        class_id: if self.settings.student.class_id.trim().is_empty() {
                                            self.current_pack
                                                .as_ref()
                                                .map(|p| p.class_id.clone())
                                                .unwrap_or_else(|| "class".to_string())
                                        } else {
                                            self.settings.student.class_id.trim().to_string()
                                        },
                                    };
                                    match homework_markdown::transcribe_outgoing_packs(&self.base_path, &defaults) {
                                        Ok(report) => {
                                            self.resync_homework();
                                            if let Some(pack) = self.current_pack.clone() {
                                                apply_pack_policy(&mut self.settings, &pack);
                                                let _ = save_settings(&self.settings, &self.base_path);
                                            }
                                            let _ = ui.label(format!(
                                                "Imported .md to outgoing and transcribed: wrote {} (skipped {}, failed {})",
                                                report.written, report.skipped, report.failed
                                            ));
                                        }
                                        Err(e) => {
                                            let _ = ui.label(format!("Transcribe failed: {e}"));
                                        }
                                    }
                                }
                            } else {
                                let dest_dir = self.base_path.join("homework").join("assigned");
                                let _ = fs::create_dir_all(&dest_dir);
                                let dest = dest_dir.join(
                                    file.file_name()
                                        .unwrap_or_else(|| std::ffi::OsStr::new("homework_pack_import.json")),
                                );
                                if let Err(e) = fs::copy(&file, &dest) {
                                    let _ = ui.label(format!("Import failed: {e}"));
                                } else if let Ok(pack) = load_pack_from_file(&dest) {
                                    apply_pack_policy(&mut self.settings, &pack);
                                    let _ = save_settings(&self.settings, &self.base_path);
                                    self.current_pack = Some(pack);
                                    self.resync_homework();
                                    let _ = ui.label(format!("Imported to {}", dest.display()));
                                }
                            }
                        }
                    }
                });

                if !self.received_homework_inbox.is_empty() {
                    ui.separator();
                    self.render_received_homework_pack_inbox(ui, "Received homework packs");
                }
                if !self.received_revision_inbox.is_empty() {
                    ui.separator();
                    self.render_received_revision_pack_inbox(ui, "Received revision packs");
                }
                if !self.received_bundle_inbox.is_empty() {
                    ui.separator();
                    self.render_received_workflow_bundle_inbox(
                        ui,
                        "Received classroom setup bundles",
                    );
                }

        if self.teacher_unlocked {
            ui.separator();
            ui.heading("Pack builder (teacher)");
            ui.label("Build a multi-assignment pack to share via the portal.");
            ui.horizontal(|ui| {
                ui.label("Assign ID");
                ui.text_edit_singleline(&mut self.draft_input.id);
                ui.label("Title");
                ui.text_edit_singleline(&mut self.draft_input.title);
            });
            ui.horizontal(|ui| {
                ui.label("Subject");
                ui.text_edit_singleline(&mut self.draft_input.subject);
                ui.label("Year / Grade");
                ui.text_edit_singleline(&mut self.draft_input.year_level);
                ui.label("Due at");
                ui.text_edit_singleline(&mut self.draft_input.due_at);
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.draft_input.allow_games, "Allow games");
                ui.checkbox(&mut self.draft_input.allow_ai_premark, "Allow AI premark");
                ui.label("Max score");
                ui.text_edit_singleline(&mut self.draft_input.max_score);
            });
            ui.label("Instructions");
            ui.text_edit_multiline(&mut self.draft_input.instructions_md);

            ui.horizontal(|ui| {
                if ui.button("Add assignment to pack").clicked() {
                    if !self.draft_input.id.trim().is_empty() {
                        let max_score = if self.draft_input.max_score.trim().is_empty() {
                            None
                        } else {
                            self.draft_input.max_score.trim().parse().ok()
                        };
                        let assignment = HomeworkAssignment {
                            id: self.draft_input.id.trim().to_string(),
                            title: self.draft_input.title.trim().to_string(),
                            subject: self.draft_input.subject.trim().to_string(),
                            year_level: self.draft_input.year_level.trim().to_string(),
                            due_at: if self.draft_input.due_at.trim().is_empty() {
                                None
                            } else {
                                Some(self.draft_input.due_at.trim().to_string())
                            },
                            instructions_md: self.draft_input.instructions_md.clone(),
                            student_printable_md: None,
                            teacher_rubric_md: None,
                            attachments: vec![],
                            allow_games: self.draft_input.allow_games,
                            allow_ai_premark: self.draft_input.allow_ai_premark,
                            max_score,
                        };
                        self.draft_assignments.push(assignment);
                        self.draft_input.id =
                            format!("hw-{:03}", self.draft_assignments.len() + 1);
                    }
                }

                if ui.button("Clear draft list").clicked() {
                    self.draft_assignments.clear();
                }

                if ui
                    .add_enabled(
                        !self.draft_assignments.is_empty(),
                        egui::Button::new("Export pack"),
                    )
                    .clicked()
                {
                    let school_id = "school";
                    let class_id = &self.settings.student.class_id;
                    match create_pack_multi(
                        &self.base_path,
                        school_id,
                        class_id,
                        self.draft_assignments.clone(),
                    ) {
                        Ok(path) => {
                            let _ = ui.label(format!("Pack saved to {}", path.display()));
                            self.resync_homework();
                            self.draft_assignments.clear();
                        }
                        Err(e) => {
                            let _ = ui.label(format!("Failed: {e}"));
                        }
                    }
                }
            });

            if !self.draft_assignments.is_empty() {
                ui.label(format!(
                    "Assignments in pack: {}",
                    self.draft_assignments.len()
                ));
            }
        } else {
            ui.separator();
            ui.colored_label(
                self.warning_color(),
                "Teacher tools are locked. Unlock via the Teacher menu to manage packs.",
            );
        }

        if let Some(pack) = self.current_pack.clone() {
            let visible_assignments = Self::unique_assignments_by_id(&pack);
            ui.separator();
            ui.label(format!(
                "Latest homework pack: {} (class {}) assignments: {}",
                pack.school_id,
                pack.class_id,
                visible_assignments.len()
            ));
            ui.horizontal(|ui| {
                ui.label("Select assignment:");
                let current = self
                    .selected_assignment_ref()
                    .map(|a| format!("{} - {}", a.id, a.title))
                    .unwrap_or_else(|| "Choose...".to_string());
                egui::ComboBox::from_id_source("home_assignment_select")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for a in &visible_assignments {
                            let label = format!("{} - {}", a.id, a.title);
                            if ui
                                .selectable_label(
                                    self.selected_assignment.as_ref() == Some(&a.id),
                                    label,
                                )
                                .clicked()
                            {
                                self.selected_assignment = Some(a.id.clone());
                            }
                        }
                    });
            });
            if let Some(assignment) = self.selected_assignment_ref() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} - {}", assignment.id, assignment.title)).strong(),
                    );
                    ui.label(format!(
                        "Subject: {} | Due: {}",
                        assignment.subject,
                        assignment.due_at.as_deref().unwrap_or("-"),
                    ));
                    if !assignment.allow_games {
                        ui.colored_label(self.warning_color(), "Games off");
                    }
                });
                self.render_assignment_materials(ui, assignment);
                ui.separator();
            }

            ui.horizontal(|ui| {
                ui.label("Finished this work and want practice from past submissions?");
                if ui
                    .add_enabled(
                        !self.revision_sources.is_empty() || !self.past_papers.is_empty(),
                        egui::Button::new("Open Revision"),
                    )
                    .clicked()
                {
                    self.open_revision_workspace();
                }
            });
            ui.add_space(6.0);
            ui.heading("Submit work");
            self.render_submission_area(ui);
        } else {
            ui.label(
                "No homework pack found. Drop a homework_pack*.json into homework/assigned/ and click Rescan.",
            );
        }

        if !self.submissions.is_empty() {
            ui.separator();
            ui.heading("Submissions found locally");
            ScrollArea::vertical()
                .id_source("home_tab_submissions_scroll")
                .max_height(200.0)
                .show(ui, |ui| {
                for row in self.submission_rows() {
                    ui.push_id(
                        (
                            "home_submission_row",
                            &row.assignment_id,
                            &row.student_id,
                            &row.submitted_at,
                        ),
                        |ui| {
                            let label = format!(
                                "{} ({}) - {} ({}) | subj: {} | score: {} | {}",
                                row.assignment_title,
                                row.assignment_id,
                                row.student_name,
                                row.student_id,
                                row.subject,
                                row.score,
                                row.feedback
                            );
                            ui.label(label).on_hover_text(format!(
                                "Assignment ID: {} | Student ID: {} | Submitted: {}",
                                row.assignment_id, row.student_id, row.submitted_at
                            ));
                        },
                    );
                }
            });
        }

                ui.add_space(12.0);
                ui.separator();
                ui.heading("Chat");
                ui.label("Use the chat bar at the bottom to message Chatty from Home.");
                self.render_chat_context_banner(ui);
                self.render_home_chat_preview(ui);
            });
    }

    fn render_chat(&mut self, ui: &mut egui::Ui) {
        ui.heading("Chat");
        ui.add_space(6.0);
        if self.sandbox_dir.is_some() {
            ui.horizontal_wrapped(|ui| {
                ui.small("Sandbox quick access:");
                if ui.button("Open scratchpad").clicked() {
                    self.open_default_sandbox_scratchpad();
                }
                if ui.button("Open ledger").clicked() {
                    self.open_default_sandbox_task_ledger();
                }
                let reopen = ui.add_enabled(
                    self.sandbox_last_working_path.is_some(),
                    egui::Button::new("Reopen last working file"),
                );
                if reopen.clicked() {
                    self.reopen_last_sandbox_working_file();
                }
            });
            ui.add_space(6.0);
        }
        let panel_height = ui.available_height();
        let sidebar_width = 320.0;
        let gap = 10.0;

        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(sidebar_width, panel_height),
                Layout::top_down(Align::Min),
                |ui| self.render_chatty_thoughts_panel(ui),
            );
            ui.add_space(gap);

            let center_width = (ui.available_width() - sidebar_width - gap).max(300.0);
            ui.allocate_ui_with_layout(
                egui::vec2(center_width, panel_height),
                Layout::top_down(Align::Min),
                |ui| {
                    self.render_chat_context_banner(ui);
                    self.render_chat_log(ui, ui.available_height());
                },
            );
            ui.add_space(gap);

            ui.allocate_ui_with_layout(
                egui::vec2(sidebar_width, panel_height),
                Layout::top_down(Align::Min),
                |ui| self.render_memory_jogger_panel(ui),
            );
        });
    }

    fn render_sandbox(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sandbox");
        ui.separator();

        let Some(dir) = self.sandbox_dir.clone() else {
            ui.label(
                "Sandbox folder not found. Create `Chatty_Sandbox/` inside the app data folder.",
            );
            return;
        };

        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Folder: {}", dir.display()));
            if ui.button("Open folder").clicked() {
                open_path_in_explorer(&dir);
            }
        });
        ui.add_space(6.0);

        if let Some(path) = self.sandbox_editor_path.clone() {
            if ensure_sandbox_save_path_within_dir(&dir, &path).is_err() {
                self.sandbox_editor_path = None;
                self.sandbox_status = "Blocked unsafe sandbox path.".to_string();
            }
        }

        ui.group(|ui| {
            ui.heading("Scratchpad");
            ui.small(
                "Persistent working notes for Chatty-EDU. The chat prompt can see this file, and Chatty-EDU can request writes/appends to it through the sandbox approval flow.",
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button("Open default scratchpad").clicked() {
                    self.open_default_sandbox_scratchpad();
                }
                if ui.button("Append memory jogger snapshot").clicked() {
                    let items = self.memory_jogger_items();
                    if items.is_empty() {
                        self.sandbox_status = "Memory jogger is empty.".to_string();
                    } else {
                        let mut snapshot =
                            format!("# Memory jogger snapshot ({})\n", Utc::now().to_rfc3339());
                        for item in items {
                            snapshot.push_str("- ");
                            snapshot.push_str(item.trim());
                            snapshot.push('\n');
                        }
                        snapshot.push('\n');
                        match sandbox_append(&dir, DEFAULT_SANDBOX_SCRATCHPAD_REL_PATH, &snapshot) {
                            Ok(path) => {
                                self.sandbox_status =
                                    format!("Appended memory jogger to {}", path.display());
                                self.open_sandbox_file_in_editor(&path);
                            }
                            Err(err) => {
                                self.sandbox_status =
                                    format!("Could not append memory jogger snapshot: {err}");
                            }
                        }
                    }
                }
            });
        });

        ui.add_space(8.0);

        ui.group(|ui| {
            ui.heading("Task Ledger");
            ui.small(
                "Structured durable state for longer tasks: current task, next step, open questions, and files touched.",
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button("Open task ledger").clicked() {
                    self.open_default_sandbox_task_ledger();
                }
                if ui.button("Seed from current context").clicked() {
                    self.seed_default_sandbox_task_ledger_from_context();
                }
            });
        });

        ui.add_space(8.0);
        ui.columns(2, |cols| {
            cols[0].heading("Files");
            ScrollArea::vertical()
                .id_source("edu_sandbox_files_scroll")
                .max_height(cols[0].available_height())
                .show(&mut cols[0], |ui| {
                    for path in list_sandbox_files(&dir) {
                        let label = path
                            .strip_prefix(&dir)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        if ui
                            .selectable_label(self.sandbox_selected.as_ref() == Some(&path), label)
                            .clicked()
                        {
                            self.open_sandbox_file_in_editor(&path);
                        }
                    }
                });

            cols[1].heading("Editor");
            let ledger_summary = read_task_ledger_summary(&dir);
            cols[1].group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Task Ledger Snapshot");
                    if let Some(summary) = ledger_summary.as_ref() {
                        if !summary.status.trim().is_empty() {
                            ui.small(format!("Status: {}", summary.status.trim()));
                        }
                    }
                    if ui.button("Open ledger").clicked() {
                        self.open_default_sandbox_task_ledger();
                    }
                });
                self.render_task_ledger_snapshot(ui, ledger_summary.as_ref());
            });
            cols[1].add_space(8.0);
            cols[1].horizontal_wrapped(|ui| {
                if ui.button("New scratch").clicked() {
                    self.sandbox_editor_path = None;
                    self.sandbox_selected = None;
                    self.sandbox_editor_text.clear();
                    self.sandbox_status = "New scratch buffer".to_string();
                }
                if ui.button("Append summary to memory jogger").clicked() {
                    self.append_editor_summary_to_memory_jogger();
                }
                if ui.button("Use as current task").clicked() {
                    self.set_task_ledger_field_from_editor(true);
                }
                if ui.button("Use as next step").clicked() {
                    self.set_task_ledger_field_from_editor(false);
                }
                if ui.button("Promote to scratchpad").clicked() {
                    self.promote_editor_text_to_scratchpad();
                }
                if ui.button("Promote to ledger notes").clicked() {
                    self.promote_editor_text_to_ledger_notes();
                }
            });
            cols[1].horizontal_wrapped(|ui| {
                if ui.button("Save as...").clicked() {
                    if let Some(path) = FileDialog::new().set_directory(&dir).save_file() {
                        match ensure_sandbox_save_path_within_dir(&dir, &path) {
                            Ok(safe_path) => match fs::write(&safe_path, &self.sandbox_editor_text)
                            {
                                Ok(_) => {
                                    self.sandbox_editor_path = Some(safe_path.clone());
                                    self.sandbox_selected = Some(safe_path.clone());
                                    self.sandbox_last_working_path = Some(safe_path.clone());
                                    self.sandbox_status = format!("Saved {}", safe_path.display());
                                }
                                Err(err) => {
                                    self.sandbox_status = format!("Save failed: {err}");
                                }
                            },
                            Err(err) => {
                                self.sandbox_status = format!("Save blocked/failed: {err}");
                            }
                        }
                    }
                }
                if ui.button("Save").clicked() {
                    if let Some(path) = self.sandbox_editor_path.clone() {
                        match ensure_sandbox_save_path_within_dir(&dir, &path) {
                            Ok(safe_path) => match fs::write(&safe_path, &self.sandbox_editor_text)
                            {
                                Ok(_) => {
                                    self.sandbox_status = format!("Saved {}", safe_path.display());
                                }
                                Err(err) => {
                                    self.sandbox_status = format!("Save failed: {err}");
                                }
                            },
                            Err(err) => {
                                self.sandbox_status = format!("Save blocked/failed: {err}");
                            }
                        }
                    } else {
                        self.sandbox_status = "No file path. Use Save as...".to_string();
                    }
                }
            });
            ScrollArea::vertical()
                .id_source("edu_sandbox_editor_scroll")
                .max_height(cols[1].available_height() - 16.0)
                .show(&mut cols[1], |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.sandbox_editor_text)
                            .desired_rows(26)
                            .code_editor(),
                    );
                });
        });

        if !self.sandbox_status.trim().is_empty() {
            ui.add_space(8.0);
            ui.small(format!("Sandbox: {}", self.sandbox_status));
        }
    }

    fn render_networking(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.networking.snapshot().clone();
        if self
            .networking_focus_flash_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.networking_focus_flash_until = None;
            self.networking_focus_section = None;
        }
        let pending_focus = self.networking_focus_pending.take();
        let highlighted_section = self.networking_focus_section;
        let highlight_until = self.networking_focus_flash_until;
        let highlight_active = |section: NetworkingFocusSection| {
            highlighted_section == Some(section)
                && highlight_until.is_some_and(|until| Instant::now() < until)
        };
        let blend_color = |base: egui::Color32, tint: egui::Color32, amount: f32| {
            let inverse = 1.0 - amount;
            egui::Color32::from_rgba_premultiplied(
                ((base.r() as f32 * inverse) + (tint.r() as f32 * amount)).round() as u8,
                ((base.g() as f32 * inverse) + (tint.g() as f32 * amount)).round() as u8,
                ((base.b() as f32 * inverse) + (tint.b() as f32 * amount)).round() as u8,
                base.a().max(244),
            )
        };
        let network_card_frame =
            |ui: &egui::Ui, tint: egui::Color32, accent: egui::Color32, selected: bool| {
                let fill_mix = if selected {
                    if ui.visuals().dark_mode {
                        0.26
                    } else {
                        0.16
                    }
                } else if ui.visuals().dark_mode {
                    0.18
                } else {
                    0.10
                };
                let stroke_mix = if selected {
                    if ui.visuals().dark_mode {
                        0.80
                    } else {
                        0.55
                    }
                } else if ui.visuals().dark_mode {
                    0.55
                } else {
                    0.35
                };
                let stroke_width = if selected { 1.6 } else { 1.0 };
                egui::Frame::group(ui.style())
                    .fill(blend_color(ui.visuals().panel_fill, tint, fill_mix))
                    .stroke(egui::Stroke::new(
                        stroke_width,
                        blend_color(
                            ui.visuals().widgets.noninteractive.bg_stroke.color,
                            accent,
                            stroke_mix,
                        ),
                    ))
                    .inner_margin(egui::Margin::same(8.0))
            };

        let local_connection_info = {
            let mut parts = vec![
                format!("Name: {}", snapshot.device_name),
                format!("Device ID: {}", snapshot.device_id),
            ];
            if let Some(port) = snapshot.listener_port {
                parts.push(format!("Listener port: {port}"));
            } else {
                parts.push("Listener: client only".to_string());
            }
            parts.push(format!(
                "Visibility: {}",
                if snapshot.available_for_connectivity {
                    "available"
                } else {
                    "hidden"
                }
            ));
            if !snapshot.local_presence.active_tab.trim().is_empty() {
                parts.push(format!(
                    "Active tab: {}",
                    snapshot.local_presence.active_tab
                ));
            }
            parts.join(" | ")
        };
        if snapshot
            .connected_peers
            .iter()
            .all(|peer| peer.connection_id != self.networking_handoff_target)
        {
            self.networking_handoff_target = snapshot
                .connected_peers
                .first()
                .map(|peer| peer.connection_id.clone())
                .unwrap_or_default();
        }

        let filter_text = self.networking_filter.trim().to_lowercase();
        let matches_filter = |name: &str, device_id: &str, address: &str, group: Option<String>| {
            if filter_text.is_empty() {
                return true;
            }
            let haystack = format!(
                "{} {} {} {}",
                name.to_lowercase(),
                device_id.to_lowercase(),
                address.to_lowercase(),
                group.unwrap_or_default().to_lowercase(),
            );
            haystack.contains(&filter_text)
        };
        let connected_visible = snapshot
            .connected_peers
            .iter()
            .filter(|peer| {
                matches_filter(
                    &self.network_display_name(&peer.device_id, &peer.device_name),
                    &peer.device_id,
                    &peer.address,
                    self.network_group_label(&peer.device_id),
                )
            })
            .collect::<Vec<_>>();
        let available_visible = snapshot
            .discovered_peers
            .iter()
            .filter(|peer| {
                peer.connected_connection_id.is_none()
                    && matches_filter(
                        &self.network_display_name(&peer.device_id, &peer.device_name),
                        &peer.device_id,
                        &format!("{}:{}", peer.address, peer.host_port),
                        self.network_group_label(&peer.device_id),
                    )
            })
            .collect::<Vec<_>>();
        let blocked_visible = snapshot
            .blocked_peers
            .iter()
            .filter(|peer| {
                matches_filter(
                    &self.network_display_name(&peer.device_id, &peer.device_name),
                    &peer.device_id,
                    &peer.address,
                    self.network_group_label(&peer.device_id),
                )
            })
            .collect::<Vec<_>>();
        let trusted_visible = snapshot
            .trusted_peers
            .iter()
            .filter(|peer| {
                matches_filter(
                    &self.network_display_name(&peer.device_id, &peer.device_name),
                    &peer.device_id,
                    &peer.address,
                    self.network_group_label(&peer.device_id),
                )
            })
            .collect::<Vec<_>>();
        let shared_room_connection_ids = snapshot
            .connected_peers
            .iter()
            .map(|peer| peer.connection_id.clone())
            .collect::<Vec<_>>();
        let delivery_visible = snapshot
            .outgoing_artifacts
            .iter()
            .filter(|artifact| {
                artifact.kind != "shared_chat_policy_json"
                    && artifact.kind != "shared_chat_message_json"
            })
            .collect::<Vec<_>>();
        let received_transfer_visible = snapshot
            .received_artifacts
            .iter()
            .filter(|artifact| {
                artifact.kind != "shared_chat_policy_json"
                    && artifact.kind != "shared_chat_message_json"
            })
            .collect::<Vec<_>>();
        let connected_keys = connected_visible
            .iter()
            .map(|peer| {
                if peer.device_id.trim().is_empty() {
                    peer.connection_id.clone()
                } else {
                    peer.device_id.clone()
                }
            })
            .collect::<Vec<_>>();
        let available_keys = available_visible
            .iter()
            .map(|peer| peer.device_id.clone())
            .collect::<Vec<_>>();
        let blocked_keys = blocked_visible
            .iter()
            .map(|peer| peer.device_id.clone())
            .collect::<Vec<_>>();
        let section_heading = |ui: &mut egui::Ui, icon: &str, color: egui::Color32, title: &str| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(icon).color(color).strong());
                ui.label(RichText::new(title).strong());
            });
        };

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                ui.heading("Networking");
                ui.label(
                    "Connect nearby Chatty-EDU instances over the local Wi-Fi / LAN. Turn one device on as the host, then scan and connect from the others.",
                );
                ui.separator();
                egui::CollapsingHeader::new("Quick help")
                    .id_source("chatty_edu_networking_quick_help")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Presets").strong());
                            if ui
                                .selectable_value(
                                    &mut self.networking_help_mode,
                                    NetworkingQuickHelpMode::Everyday,
                                    "Everyday",
                                )
                                .clicked()
                            {
                                self.focus_networking_section(NetworkingFocusSection::DeviceList);
                            }
                            if ui
                                .selectable_value(
                                    &mut self.networking_help_mode,
                                    NetworkingQuickHelpMode::TeacherFlow,
                                    "Teacher mode",
                                )
                                .clicked()
                            {
                                self.focus_networking_section(NetworkingFocusSection::DeviceList);
                            }
                            if ui
                                .selectable_value(
                                    &mut self.networking_help_mode,
                                    NetworkingQuickHelpMode::ApprovalFirst,
                                    "Approval first",
                                )
                                .clicked()
                            {
                                let target = if snapshot.pending_requests.is_empty() {
                                    NetworkingFocusSection::Controls
                                } else {
                                    NetworkingFocusSection::PendingRequests
                                };
                                self.focus_networking_section(target);
                            }
                        });
                        ui.add_space(4.0);

                        let help_rows = match self.networking_help_mode {
                            NetworkingQuickHelpMode::Everyday => vec![
                                (
                                    "[HOST]",
                                    "Turn on `Make available for connectivity` on the teacher or host device.",
                                ),
                                (
                                    "[SCAN]",
                                    "Click `Refresh discovery` on the other machine, then `Connect` when it appears.",
                                ),
                                (
                                    "[NAME]",
                                    "Click a device name to rename it locally, or click `+ Group` to tag it by class, table, or role.",
                                ),
                                (
                                    "[FIND]",
                                    "Use `Find` to search by name, device ID, address, or group label.",
                                ),
                                (
                                    "[FAST]",
                                    "`Select Connected` is usually the fastest way to act on the active classroom set.",
                                ),
                                (
                                    "[LANES]",
                                    "Use `Push Pack` for homework, `Push Revision` for revision material, and `Push Setup` when you want to share the classroom app setup itself.",
                                ),
                                (
                                    "[ROOM]",
                                    "Use `Shared room chat` when you want a class room with talking-stick turns and AI-on/off rules instead of separate local chats.",
                                ),
                                (
                                    "[PAIR]",
                                    "Use `Export trusted list` / `Import trusted list` for remembered classroom pairings, and `Export blocked list` / `Import blocked list` when you want another teacher machine to inherit the same deny rules.",
                                ),
                                (
                                    "[SYNC]",
                                    "If a nearby EDU machine shows up but refuses to talk cleanly, check the `Compatibility note` line to spot protocol/version mismatch quickly.",
                                ),
                            ],
                            NetworkingQuickHelpMode::TeacherFlow => vec![
                                (
                                    "[CLASS]",
                                    "Use this when you are managing a room and want a quick read on which devices are connected, available, or blocked.",
                                ),
                                (
                                    "[TURN]",
                                    "Use `Pass Stick` when one learner should take the lead for the next step.",
                                ),
                                (
                                    "[PACK]",
                                    "Bulk actions like `Push Pack`, `Games Off`, or `Free Time` are meant for the checked connected set.",
                                ),
                                (
                                    "[SETUP]",
                                    "Use `Push Setup` when you want student devices to mirror the current classroom settings and lesson-ready EDU setup without sending logs or personal history.",
                                ),
                                (
                                    "[ROOM]",
                                    "Use the `Shared room chat` controls below when the class needs one orderly shared conversation instead of lots of individual side chats.",
                                ),
                                (
                                    "[LABEL]",
                                    "Rename devices and add group labels early so larger classroom lists stay readable later.",
                                ),
                                (
                                    "[CHECK]",
                                    "Use `Copy info` before acting if several nearby devices look similar and you want to confirm the right one.",
                                ),
                                (
                                    "[PAIR]",
                                    "Export a trusted classroom list when you want another teacher machine to inherit the same remembered device approvals.",
                                ),
                            ],
                            NetworkingQuickHelpMode::ApprovalFirst => vec![
                                (
                                    "[LOCK]",
                                    "Turn off `Allow unknown devices` if you want EDU to ask before new devices can join.",
                                ),
                                (
                                    "[QUEUE]",
                                    "Unknown device requests appear above the device list, where you can Allow, Deny, or Block them.",
                                ),
                                (
                                    "[BLOCK]",
                                    "`Block` disconnects a device now and keeps it out until you deliberately unblock it later.",
                                ),
                                (
                                    "[REVIEW]",
                                    "Use `Copy ID` or `Copy info` before allowing a new device if you need to confirm which classroom machine it is.",
                                ),
                                (
                                    "[INBOX]",
                                    "Received classroom setup bundles land in their own inbox first, so you can preview and apply them deliberately instead of having them take over immediately.",
                                ),
                                (
                                    "[ROOM]",
                                    "Use `Broadcast current room policy` if you want the class to share the same talking-stick and AI rules before discussion starts.",
                                ),
                                (
                                    "[RESET]",
                                    "Blocked devices stay in their own section so you can review or unblock them calmly instead of losing track.",
                                ),
                                (
                                    "[PAIR]",
                                    "Trusted and blocked lists are portable now, so you can import a known-good classroom policy set instead of rebuilding it by hand.",
                                ),
                            ],
                        };
                        for (tag, body) in help_rows {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(tag).monospace().strong());
                                ui.small(body);
                            });
                        }
                    });

                let controls_highlight = highlight_active(NetworkingFocusSection::Controls);
                let controls = egui::Frame::group(ui.style())
                    .fill(if controls_highlight {
                        egui::Color32::from_rgb(246, 250, 255)
                    } else {
                        ui.visuals().panel_fill
                    })
                    .stroke(if controls_highlight {
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(70, 110, 180))
                    } else {
                        ui.visuals().widgets.noninteractive.bg_stroke
                    })
                    .show(ui, |ui| {
                        section_heading(
                            ui,
                            "[CTL]",
                            egui::Color32::from_rgb(70, 110, 180),
                            "Network controls",
                        );
                        if controls_highlight {
                            ui.small(
                                RichText::new("Focused by Quick help")
                                    .strong()
                                    .color(egui::Color32::from_rgb(70, 110, 180)),
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            let mut available = snapshot.available_for_connectivity;
                            if ui
                                .checkbox(&mut available, "Make available for connectivity")
                                .changed()
                            {
                                self.networking.set_available(available);
                            }
                            let mut allow_unknown = snapshot.allow_unknown_devices;
                            if ui
                                .checkbox(&mut allow_unknown, "Allow unknown devices")
                                .changed()
                            {
                                self.networking.set_allow_unknown_devices(allow_unknown);
                                self.settings.network_allow_unknown_devices = allow_unknown;
                                self.persist_network_settings();
                            }
                            let mut allow_shared_lukewarm =
                                self.settings.network_allow_shared_lukewarm_context;
                            if ui
                                .checkbox(
                                    &mut allow_shared_lukewarm,
                                    "Allow shared luke warm context",
                                )
                                .changed()
                            {
                                self.settings.network_allow_shared_lukewarm_context =
                                    allow_shared_lukewarm;
                                self.persist_network_settings();
                            }
                            if ui.button("Refresh discovery").clicked() {
                                self.networking.refresh_discovery();
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            let has_trusted = !self.settings.network_trusted_devices.is_empty();
                            let has_blocked = !self.settings.network_blocked_devices.is_empty();
                            if ui
                                .add_enabled(
                                    has_trusted,
                                    egui::Button::new("Export trusted list"),
                                )
                                .clicked()
                            {
                                self.export_trusted_peer_list();
                            }
                            if ui.button("Import trusted list").clicked() {
                                self.import_trusted_peer_list();
                            }
                            if ui
                                .add_enabled(
                                    has_blocked,
                                    egui::Button::new("Export blocked list"),
                                )
                                .clicked()
                            {
                                self.export_blocked_peer_list();
                            }
                            if ui.button("Import blocked list").clicked() {
                                self.import_blocked_peer_list();
                            }
                            if !has_trusted {
                                ui.small(
                                    "Trust a few regular classroom devices first if you want to export a reusable trust list.",
                                );
                            } else if !has_blocked {
                                ui.small(
                                    "Blocked lists are handy when another teacher machine should inherit the same deny rules.",
                                );
                            }
                        });
                    });
                if pending_focus == Some(NetworkingFocusSection::Controls) {
                    controls.response.scroll_to_me(Some(Align::Center));
                }

                if !snapshot.pending_requests.is_empty() {
                    ui.add_space(8.0);
                    let pending_highlight = highlight_active(NetworkingFocusSection::PendingRequests);
                    let pending = egui::Frame::group(ui.style())
                        .fill(if pending_highlight {
                            egui::Color32::from_rgb(255, 248, 240)
                        } else {
                            ui.visuals().panel_fill
                        })
                        .stroke(if pending_highlight {
                            egui::Stroke::new(1.5, egui::Color32::from_rgb(190, 110, 30))
                        } else {
                            ui.visuals().widgets.noninteractive.bg_stroke
                        })
                        .show(ui, |ui| {
                        section_heading(
                            ui,
                            "[REQ]",
                            egui::Color32::from_rgb(190, 110, 30),
                            "Pending device requests",
                        );
                        if pending_highlight {
                            ui.small(
                                RichText::new("Focused by Quick help")
                                    .strong()
                                    .color(egui::Color32::from_rgb(190, 110, 30)),
                            );
                        }
                        for request in &snapshot.pending_requests {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!(
                                    "Unknown device {} [{}] requesting connection from {}.",
                                    request.device_name, request.device_id, request.address
                                ));
                                ui.small(format!("{}s ago", request.requested_secs_ago));
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Allow").clicked() {
                                    self.networking.allow_pending_peer(&request.device_id);
                                    self.networking_status = Some(format!(
                                        "Allowed {}. Ask it to reconnect.",
                                        request.device_name
                                    ));
                                }
                                if ui.button("Trust").clicked() {
                                    self.trust_network_peer(
                                        &request.device_id,
                                        &request.device_name,
                                    );
                                }
                                if ui.button("Deny").clicked() {
                                    self.networking.deny_pending_peer(&request.device_id);
                                    self.networking_status =
                                        Some(format!("Denied {} for now.", request.device_name));
                                }
                                if ui.button("Block").clicked() {
                                    self.block_network_peer(
                                        &request.device_id,
                                        &request.device_name,
                                    );
                                }
                            });
                            ui.separator();
                        }
                    });
                    if pending_focus == Some(NetworkingFocusSection::PendingRequests) {
                        pending.response.scroll_to_me(Some(Align::Center));
                    }
                }

                ui.add_space(8.0);
                ui.group(|ui| {
                    section_heading(
                        ui,
                        "[ME]",
                        egui::Color32::from_rgb(70, 110, 180),
                        "This device",
                    );
                    ui.label(format!("Name: {}", snapshot.device_name));
                    ui.horizontal(|ui| {
                        ui.label("Device ID:");
                        ui.monospace(&snapshot.device_id);
                        if ui.button("Copy device ID").clicked() {
                            ui.ctx().copy_text(snapshot.device_id.clone());
                            self.networking_status = Some("Copied local device ID.".to_string());
                        }
                        if ui.button("Copy connection info").clicked() {
                            ui.ctx().copy_text(local_connection_info.clone());
                            self.networking_status = Some("Copied local connection info.".to_string());
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Edit name:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.networking_device_name_input)
                                .desired_width(260.0)
                                .hint_text("e.g. Staffroom Laptop"),
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Save name").clicked() {
                            let trimmed = self.networking_device_name_input.trim().to_string();
                            self.networking.set_device_name(&trimmed);
                            self.settings.network_device_name = trimmed;
                            match save_settings(&self.settings, &self.base_path) {
                                Ok(()) => self.networking_status =
                                    Some("Saved networking device name.".to_string()),
                                Err(err) => self.networking_status =
                                    Some(format!("Could not save device name: {err}")),
                            }
                            self.networking_device_name_input =
                                self.networking.snapshot().device_name.clone();
                        }
                        if ui.button("Reset default").clicked() {
                            self.networking.set_device_name("");
                            self.settings.network_device_name.clear();
                            match save_settings(&self.settings, &self.base_path) {
                                Ok(()) => self.networking_status =
                                    Some("Reset networking device name.".to_string()),
                                Err(err) => self.networking_status =
                                    Some(format!("Could not save device name: {err}")),
                            }
                            self.networking_device_name_input =
                                self.networking.snapshot().device_name.clone();
                        }
                    });
                    ui.label(format!(
                        "Visibility: {}",
                        if snapshot.available_for_connectivity {
                            "Available on local network"
                        } else {
                            "Hidden / client only"
                        }
                    ));
                    if let Some(port) = snapshot.listener_port {
                        ui.label(format!("Host port: {port}"));
                    }
                    if !snapshot.local_presence.active_tab.trim().is_empty() {
                        ui.label(format!("Shared active tab: {}", snapshot.local_presence.active_tab));
                    }
                    if !snapshot.local_presence.runtime_status.trim().is_empty() {
                        ui.label(format!(
                            "Shared status: {}",
                            snapshot.local_presence.runtime_status
                        ));
                    }
                    if !snapshot.local_presence.model_label.trim().is_empty() {
                        ui.label(format!(
                            "Shared model: {}",
                            snapshot.local_presence.model_label
                        ));
                    }
                    if !snapshot.status.trim().is_empty() {
                        ui.label(format!("Status: {}", snapshot.status));
                    }
                    if !snapshot.protocol_notice.trim().is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(190, 110, 30),
                            format!("Compatibility note: {}", snapshot.protocol_notice),
                        );
                    }
                    if !snapshot.last_error.trim().is_empty() {
                        ui.colored_label(
                            self.warning_color(),
                            format!("Last error: {}", snapshot.last_error),
                        );
                    }
                if let Some(status) = &self.networking_status {
                    ui.small(status);
                }
                });

                ui.add_space(12.0);
                let device_list_highlight = highlight_active(NetworkingFocusSection::DeviceList);
                let device_list = egui::Frame::group(ui.style())
                    .fill(if device_list_highlight {
                        egui::Color32::from_rgb(244, 250, 244)
                    } else {
                        ui.visuals().panel_fill
                    })
                    .stroke(if device_list_highlight {
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(70, 140, 90))
                    } else {
                        ui.visuals().widgets.noninteractive.bg_stroke
                    })
                    .show(ui, |ui| {
                    section_heading(
                        ui,
                        "[ACT]",
                        egui::Color32::from_rgb(70, 140, 90),
                        "Classroom actions",
                    );
                    if device_list_highlight {
                        ui.small(
                            RichText::new("Focused by Quick help")
                                .strong()
                                .color(egui::Color32::from_rgb(70, 140, 90)),
                        );
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Select All").clicked() {
                            self.networking_selected_devices = connected_keys
                                .iter()
                                .chain(available_keys.iter())
                                .chain(blocked_keys.iter())
                                .cloned()
                                .collect();
                        }
                        if ui.button("Deselect All").clicked() {
                            self.networking_selected_devices.clear();
                        }
                        if ui.button("Select Connected").clicked() {
                            self.networking_selected_devices = connected_keys.iter().cloned().collect();
                        }
                        if ui.button("Select Available").clicked() {
                            self.networking_selected_devices = available_keys.iter().cloned().collect();
                        }
                        ui.separator();
                        ui.label("Find:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.networking_filter)
                                .desired_width(200.0)
                                .hint_text("find Jim"),
                        );
                    });
                    let selected_count = self.networking_selected_devices.len();
                    let selected_connections = connected_visible
                        .iter()
                        .filter(|peer| {
                            let key = if peer.device_id.trim().is_empty() {
                                peer.connection_id.clone()
                            } else {
                                peer.device_id.clone()
                            };
                            self.networking_selected_devices.contains(&key)
                        })
                        .collect::<Vec<_>>();
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(selected_count > 0, egui::Button::new("Games On")).clicked()
                        {
                            for peer in &selected_connections {
                                self.networking.send_handoff(
                                    &peer.connection_id,
                                    "Classroom mode",
                                    "Games are on for free-choice time.",
                                );
                            }
                        }
                        if ui.add_enabled(selected_count > 0, egui::Button::new("Games Off")).clicked()
                        {
                            for peer in &selected_connections {
                                self.networking.send_handoff(
                                    &peer.connection_id,
                                    "Classroom mode",
                                    "Games are off. Please return to classwork.",
                                );
                            }
                        }
                        if ui.add_enabled(selected_count > 0, egui::Button::new("Free Time")).clicked()
                        {
                            for peer in &selected_connections {
                                self.networking.send_handoff(
                                    &peer.connection_id,
                                    "Classroom mode",
                                    "Free time is active. Finish your current step, then you may switch.",
                                );
                            }
                        }
                        if ui.add_enabled(selected_count > 0, egui::Button::new("Push Pack")).clicked()
                        {
                            if let Some(pack) = &self.current_pack {
                                match serde_json::to_string_pretty(pack) {
                                    Ok(text) => {
                                        let label = format!(
                                            "{} pack ({})",
                                            pack.class_id,
                                            pack.assignments.len()
                                        );
                                        let summary = format!(
                                            "{} assignment(s) for class {}",
                                            pack.assignments.len(),
                                            pack.class_id
                                        );
                                        let file_name = format!(
                                            "homework_pack_{}_network.json",
                                            slugify_filename(&pack.class_id, "class")
                                        );
                                        for peer in &selected_connections {
                                            self.networking.send_artifact(
                                                &peer.connection_id,
                                                "homework_pack_json",
                                                &label,
                                                None,
                                                &summary,
                                                &file_name,
                                                &text,
                                            );
                                        }
                                        self.networking_status = Some(format!(
                                            "Sent the current pack to {} selected device(s).",
                                            selected_connections.len()
                                        ));
                                    }
                                    Err(err) => {
                                        self.networking_status = Some(format!(
                                            "Could not serialize the current pack: {}",
                                            err
                                        ));
                                    }
                                }
                            } else {
                                self.networking_status =
                                    Some("There is no current pack loaded to send.".to_string());
                            }
                        }
                        if ui
                            .add_enabled(selected_count > 0, egui::Button::new("Push Revision"))
                            .clicked()
                        {
                            match find_latest_revision_pack_markdown(&self.base_path) {
                                Ok(Some((path, text))) => {
                                    let label = path
                                        .file_stem()
                                        .and_then(|stem| stem.to_str())
                                        .unwrap_or("revision_pack")
                                        .replace('_', " ");
                                    let summary = format!(
                                        "Revision pack markdown from {}",
                                        path.file_name()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or("revision pack")
                                    );
                                    let file_name = path
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .unwrap_or("revision_pack.md")
                                        .to_string();
                                    for peer in &selected_connections {
                                        self.networking.send_artifact(
                                            &peer.connection_id,
                                            "revision_pack_markdown",
                                            &label,
                                            None,
                                            &summary,
                                            &file_name,
                                            &text,
                                        );
                                    }
                                    self.networking_status = Some(format!(
                                        "Sent the latest revision pack to {} selected device(s).",
                                        selected_connections.len()
                                    ));
                                }
                                Ok(None) => {
                                    self.networking_status = Some(
                                        "There is no revision pack markdown file to send yet."
                                            .to_string(),
                                    );
                                }
                                Err(err) => {
                                    self.networking_status = Some(format!(
                                        "Could not read the latest revision pack: {}",
                                        err
                                    ));
                                }
                            }
                        }
                        if ui
                            .add_enabled(selected_count > 0, egui::Button::new("Push Setup"))
                            .clicked()
                        {
                            let bundle = self.build_current_workflow_bundle();
                            let summary = if bundle.summary.trim().is_empty() {
                                format!(
                                    "Classroom setup | mode {} | Janet {} | games in class {}",
                                    bundle.teacher_mode,
                                    if bundle.janet.enabled { "on" } else { "off" },
                                    if bundle.game.games_in_class_allowed {
                                        "allowed"
                                    } else {
                                        "off"
                                    }
                                )
                            } else {
                                bundle.summary.trim().to_string()
                            };
                            match serde_json::to_string_pretty(&bundle) {
                                Ok(text) => {
                                    let label = if bundle.label.trim().is_empty() {
                                        "Classroom setup".to_string()
                                    } else {
                                        bundle.label.trim().to_string()
                                    };
                                    let file_name = format!(
                                        "workflow_bundle_{}.json",
                                        slugify_filename(&label, "workflow_bundle")
                                    );
                                    for peer in &selected_connections {
                                        self.networking.send_artifact(
                                            &peer.connection_id,
                                            "workflow_bundle_json",
                                            &label,
                                            None,
                                            &summary,
                                            &file_name,
                                            &text,
                                        );
                                    }
                                    self.networking_status = Some(format!(
                                        "Sent classroom setup bundle to {} selected device(s).",
                                        selected_connections.len()
                                    ));
                                }
                                Err(err) => {
                                    self.networking_status = Some(format!(
                                        "Could not serialize the classroom setup bundle: {}",
                                        err
                                    ));
                                }
                            }
                        }
                        if ui
                            .add_enabled(selected_count > 0, egui::Button::new("Push Luke Warm"))
                            .clicked()
                        {
                            let context = self.build_current_lukewarm_share();
                            if context.context_text.trim().is_empty() {
                                self.networking_status = Some(
                                    "There is no current luke warm EDU context ready to share yet."
                                        .to_string(),
                                );
                            } else {
                                match serde_json::to_string_pretty(&context) {
                                    Ok(text) => {
                                        let file_name = format!(
                                            "lukewarm_context_{}.json",
                                            slugify_filename(&context.label, "lukewarm_context")
                                        );
                                        for peer in &selected_connections {
                                            self.networking.send_artifact(
                                                &peer.connection_id,
                                                "lukewarm_context_json",
                                                &context.label,
                                                None,
                                                &context.summary,
                                                &file_name,
                                                &text,
                                            );
                                        }
                                        self.networking_status = Some(format!(
                                            "Sent shared luke warm context to {} selected device(s).",
                                            selected_connections.len()
                                        ));
                                    }
                                    Err(err) => {
                                        self.networking_status = Some(format!(
                                            "Could not serialize the luke warm context: {}",
                                            err
                                        ));
                                    }
                                }
                            }
                        }
                        if ui
                            .add_enabled(selected_count > 0, egui::Button::new("Boot Selected"))
                            .clicked()
                        {
                            for peer in &selected_connections {
                                self.networking.disconnect_connection(&peer.connection_id);
                            }
                        }
                        if ui
                            .add_enabled(selected_count > 0, egui::Button::new("Block Selected"))
                            .clicked()
                        {
                            let mut blocked_count = 0usize;
                            for peer in &connected_visible {
                                let key = if peer.device_id.trim().is_empty() {
                                    peer.connection_id.clone()
                                } else {
                                    peer.device_id.clone()
                                };
                                if self.networking_selected_devices.contains(&key)
                                    && !peer.device_id.trim().is_empty()
                                {
                                    self.block_network_peer(&peer.device_id, &peer.device_name);
                                    blocked_count += 1;
                                }
                            }
                            for peer in &available_visible {
                                if self.networking_selected_devices.contains(&peer.device_id) {
                                    self.block_network_peer(&peer.device_id, &peer.device_name);
                                    blocked_count += 1;
                                }
                            }
                            if blocked_count > 0 {
                                self.networking_status = Some(format!(
                                    "Blocked {} selected device(s).",
                                    blocked_count
                                ));
                            }
                        }
                    });
                    ui.small(format!(
                        "Connected: {} | Available: {} | Blocked: {} | Selected: {}",
                        connected_visible.len(),
                        available_visible.len(),
                        blocked_visible.len(),
                        selected_count
                    ));
                    ui.small(
                        "Tip: click a device name to rename it, and click the group chip to set a class / group label.",
                    );
                    ui.add_space(6.0);
                    ui.label(RichText::new("Classroom setup bundle").strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut self.networking_bundle_label)
                            .hint_text("Bundle title..."),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut self.networking_bundle_summary)
                            .desired_rows(2)
                            .hint_text("What is this setup for?"),
                    );
                    ui.small(
                        "This shares lesson-wide setup preferences and model hints. Homework and revision content still travel through their own dedicated pack lanes.",
                    );
                });
                if pending_focus == Some(NetworkingFocusSection::DeviceList) {
                    device_list.response.scroll_to_me(Some(Align::Center));
                }

                ui.add_space(8.0);
                let selected_connected_count = connected_visible
                    .iter()
                    .filter(|peer| {
                        let key = if peer.device_id.trim().is_empty() {
                            peer.connection_id.clone()
                        } else {
                            peer.device_id.clone()
                        };
                        self.networking_selected_devices.contains(&key)
                    })
                    .count();
                let selected_available_count = available_visible
                    .iter()
                    .filter(|peer| self.networking_selected_devices.contains(&peer.device_id))
                    .count();
                let selected_blocked_count = blocked_visible
                    .iter()
                    .filter(|peer| self.networking_selected_devices.contains(&peer.device_id))
                    .count();
                let render_selection_chip =
                    |ui: &mut egui::Ui, label: &str, count: usize, tint: egui::Color32| {
                        let fill = if count > 0 {
                            blend_color(ui.visuals().panel_fill, tint, 0.18)
                        } else {
                            blend_color(
                                ui.visuals().panel_fill,
                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                                0.06,
                            )
                        };
                        let stroke = if count > 0 {
                            blend_color(
                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                                tint,
                                0.55,
                            )
                        } else {
                            ui.visuals().widgets.noninteractive.bg_stroke.color
                        };
                        egui::Frame::group(ui.style())
                            .fill(fill)
                            .stroke(egui::Stroke::new(1.0, stroke))
                            .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.small(egui::RichText::new(label).strong());
                                    ui.small(count.to_string());
                                });
                            });
                    };
                ui.horizontal_wrapped(|ui| {
                    render_selection_chip(
                        ui,
                        "Selected",
                        self.networking_selected_devices.len(),
                        egui::Color32::from_rgb(70, 110, 180),
                    );
                    render_selection_chip(
                        ui,
                        "Connected",
                        selected_connected_count,
                        egui::Color32::from_rgb(70, 110, 180),
                    );
                    render_selection_chip(
                        ui,
                        "Available",
                        selected_available_count,
                        egui::Color32::from_rgb(70, 140, 90),
                    );
                    render_selection_chip(
                        ui,
                        "Blocked",
                        selected_blocked_count,
                        egui::Color32::from_rgb(160, 60, 60),
                    );
                    if !self.networking_selected_devices.is_empty() {
                        if ui.small_button("Clear selection").clicked() {
                            self.networking_selected_devices.clear();
                        }
                        ui.small("Bulk actions apply to the checked devices.");
                    }
                });
                ui.add_space(6.0);
                ui.columns(2, |cols| {
                    section_heading(
                        &mut cols[0],
                        "[AVL]",
                        egui::Color32::from_rgb(70, 140, 90),
                        &format!("Available ({})", available_visible.len()),
                    );
                    cols[0].small("Visible on the network but not currently connected.");
                    cols[0].add_space(6.0);

                    if available_visible.is_empty() {
                        cols[0].label("(none found yet)");
                    } else {
                        ScrollArea::vertical()
                            .id_source("edu_network_discovered_scroll")
                            .max_height(360.0)
                            .show(&mut cols[0], |ui| {
                                for peer in &available_visible {
                                    let key = peer.device_id.clone();
                                    let selected_initial =
                                        self.networking_selected_devices.contains(&key);
                                    let card = network_card_frame(
                                        ui,
                                        egui::Color32::from_rgb(223, 241, 228),
                                        egui::Color32::from_rgb(70, 140, 90),
                                        selected_initial,
                                    )
                                    .show(ui, |ui| {
                                        let display_name =
                                            self.network_display_name(&peer.device_id, &peer.device_name);
                                        let can_persist_identity = !peer.device_id.trim().is_empty();
                                        let is_trusted = self.network_is_trusted(&peer.device_id);
                                        let alias_editing = self
                                            .networking_alias_edit_device
                                            .as_deref()
                                            == Some(peer.device_id.as_str());
                                        let group_editing = self
                                            .networking_group_edit_device
                                            .as_deref()
                                            == Some(peer.device_id.as_str());

                                        ui.horizontal_wrapped(|ui| {
                                            let mut selected =
                                                self.networking_selected_devices.contains(&key);
                                            if ui.checkbox(&mut selected, "").changed() {
                                                if selected {
                                                    self.networking_selected_devices.insert(key.clone());
                                                } else {
                                                    self.networking_selected_devices.remove(&key);
                                                }
                                            }
                                            if can_persist_identity {
                                                if ui
                                                    .link(RichText::new(display_name.clone()).strong())
                                                    .clicked()
                                                {
                                                    self.begin_network_alias_edit(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                            } else {
                                                ui.strong(display_name.clone());
                                            }
                                            if is_trusted {
                                                ui.small(
                                                    RichText::new("Trusted")
                                                        .color(egui::Color32::from_rgb(110, 80, 170))
                                                        .strong(),
                                                );
                                            }
                                            ui.small(format!("{}:{}", peer.address, peer.host_port));
                                            if let Some(group) =
                                                self.network_group_label(&peer.device_id)
                                            {
                                                if ui.small_button(format!("Group: {group}")).clicked()
                                                {
                                                    self.begin_network_group_edit(&peer.device_id);
                                                }
                                            } else if can_persist_identity
                                                && ui.small_button("+ Group").clicked()
                                            {
                                                self.begin_network_group_edit(&peer.device_id);
                                            }
                                        });
                                        if alias_editing {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label("Rename:");
                                                ui.add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.networking_alias_input,
                                                    )
                                                    .desired_width(220.0)
                                                    .hint_text("Student laptop 3"),
                                                );
                                                if ui.button("Save").clicked() {
                                                    self.save_network_alias_edit(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                                if ui.button("Cancel").clicked() {
                                                    self.cancel_network_alias_edit();
                                                }
                                            });
                                        }
                                        if group_editing {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label("Group:");
                                                ui.add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.networking_group_input,
                                                    )
                                                    .desired_width(180.0)
                                                    .hint_text("e.g. Maths A"),
                                                );
                                                if ui.button("Save group").clicked() {
                                                    self.save_network_group_edit(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                                if ui.button("Clear").clicked() {
                                                    self.networking_group_input.clear();
                                                    self.save_network_group_edit(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                                if ui.button("Cancel").clicked() {
                                                    self.cancel_network_group_edit();
                                                }
                                            });
                                        }
                                        let mut status_line =
                                            vec![format!("Seen {}s ago", peer.last_seen_secs_ago)];
                                        if is_trusted {
                                            status_line.push("Trusted".to_string());
                                        }
                                        if let Some(group) = self.network_group_label(&peer.device_id) {
                                            status_line.push(format!("Group: {group}"));
                                        }
                                        ui.small(status_line.join(" | "));
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.button("Connect").clicked() {
                                                self.networking.connect_peer(&peer.device_id);
                                            }
                                            if can_persist_identity {
                                                if is_trusted {
                                                    if ui.button("Untrust").clicked() {
                                                        self.untrust_network_peer(
                                                            &peer.device_id,
                                                            &peer.device_name,
                                                        );
                                                    }
                                                } else if ui.button("Trust").clicked() {
                                                    self.trust_network_peer(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                            }
                                            if ui.button("Block").clicked() {
                                                self.block_network_peer(
                                                    &peer.device_id,
                                                    &peer.device_name,
                                                );
                                            }
                                            if ui.small_button("Copy ID").clicked() {
                                                ui.ctx().copy_text(peer.device_id.clone());
                                                self.networking_status = Some(format!(
                                                    "Copied device ID for {}.",
                                                    display_name
                                                ));
                                            }
                                            if ui.small_button("Copy info").clicked() {
                                                ui.ctx().copy_text(format!(
                                                    "Name: {} | Device ID: {} | Address: {}:{} | Seen: {}s ago",
                                                    display_name,
                                                    peer.device_id,
                                                    peer.address,
                                                    peer.host_port,
                                                    peer.last_seen_secs_ago
                                                ));
                                                self.networking_status = Some(format!(
                                                    "Copied connection info for {}.",
                                                    display_name
                                                ));
                                            }
                                        });
                                    });
                                    if card.response.hovered() {
                                        ui.painter().rect_stroke(
                                            card.response.rect.expand(1.0),
                                            6.0,
                                            egui::Stroke::new(
                                                if selected_initial { 1.9 } else { 1.35 },
                                                blend_color(
                                                    ui.visuals()
                                                        .widgets
                                                        .noninteractive
                                                        .bg_stroke
                                                        .color,
                                                    egui::Color32::from_rgb(70, 140, 90),
                                                    if selected_initial { 0.78 } else { 0.60 },
                                                ),
                                            ),
                                        );
                                    }
                                    ui.add_space(6.0);
                                }
                            });
                    }

                    section_heading(
                        &mut cols[1],
                        "[CON]",
                        egui::Color32::from_rgb(70, 110, 180),
                        &format!("Connected ({})", connected_visible.len()),
                    );
                    cols[1].small("Devices currently in session.");
                    cols[1].add_space(6.0);

                    if connected_visible.is_empty() {
                        cols[1].label("(no active connections)");
                    } else {
                        ScrollArea::vertical()
                            .id_source("edu_network_connected_scroll")
                            .max_height(360.0)
                            .show(&mut cols[1], |ui| {
                                for peer in &connected_visible {
                                    let key = if peer.device_id.trim().is_empty() {
                                        peer.connection_id.clone()
                                    } else {
                                        peer.device_id.clone()
                                    };
                                    let selected_initial =
                                        self.networking_selected_devices.contains(&key);
                                    let card = network_card_frame(
                                        ui,
                                        egui::Color32::from_rgb(224, 234, 250),
                                        egui::Color32::from_rgb(70, 110, 180),
                                        selected_initial,
                                    )
                                    .show(ui, |ui| {
                                        let display_name =
                                            self.network_display_name(&peer.device_id, &peer.device_name);
                                        let can_persist_identity = !peer.device_id.trim().is_empty();
                                        let is_trusted = self.network_is_trusted(&peer.device_id);
                                        let alias_editing = self
                                            .networking_alias_edit_device
                                            .as_deref()
                                            == Some(peer.device_id.as_str());
                                        let group_editing = self
                                            .networking_group_edit_device
                                            .as_deref()
                                            == Some(peer.device_id.as_str());

                                        ui.horizontal_wrapped(|ui| {
                                            let mut selected =
                                                self.networking_selected_devices.contains(&key);
                                            if ui.checkbox(&mut selected, "").changed() {
                                                if selected {
                                                    self.networking_selected_devices.insert(key.clone());
                                                } else {
                                                    self.networking_selected_devices.remove(&key);
                                                }
                                            }
                                            if can_persist_identity {
                                                if ui
                                                    .link(RichText::new(display_name.clone()).strong())
                                                    .clicked()
                                                {
                                                    self.begin_network_alias_edit(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                            } else {
                                                ui.strong(display_name.clone());
                                            }
                                            if is_trusted {
                                                ui.small(
                                                    RichText::new("Trusted")
                                                        .color(egui::Color32::from_rgb(110, 80, 170))
                                                        .strong(),
                                                );
                                            }
                                            ui.small(if peer.inbound { "Inbound" } else { "Outbound" });
                                            if self.networking_turn_holder.as_deref() == Some(key.as_str()) {
                                                ui.small("Turn holder");
                                            }
                                            if let Some(group) =
                                                self.network_group_label(&peer.device_id)
                                            {
                                                if ui.small_button(format!("Group: {group}")).clicked()
                                                {
                                                    self.begin_network_group_edit(&peer.device_id);
                                                }
                                            } else if can_persist_identity
                                                && ui.small_button("+ Group").clicked()
                                            {
                                                self.begin_network_group_edit(&peer.device_id);
                                            }
                                        });
                                        if alias_editing {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label("Rename:");
                                                ui.add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.networking_alias_input,
                                                    )
                                                    .desired_width(220.0)
                                                    .hint_text("Front Row Tablet"),
                                                );
                                                if ui.button("Save").clicked() {
                                                    self.save_network_alias_edit(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                                if ui.button("Cancel").clicked() {
                                                    self.cancel_network_alias_edit();
                                                }
                                            });
                                        }
                                        if group_editing {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label("Group:");
                                                ui.add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.networking_group_input,
                                                    )
                                                    .desired_width(180.0)
                                                    .hint_text("e.g. Reading Circle"),
                                                );
                                                if ui.button("Save group").clicked() {
                                                    self.save_network_group_edit(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                                if ui.button("Clear").clicked() {
                                                    self.networking_group_input.clear();
                                                    self.save_network_group_edit(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                                if ui.button("Cancel").clicked() {
                                                    self.cancel_network_group_edit();
                                                }
                                            });
                                        }
                                        let mut status_line = vec![peer.status_summary.clone()];
                                        if is_trusted {
                                            status_line.push("Trusted".to_string());
                                        }
                                        if let Some(group) = self.network_group_label(&peer.device_id) {
                                            status_line.push(format!("Group: {group}"));
                                        }
                                        ui.small(status_line.join(" | "));
                                        ui.label(format!("Address: {}", peer.address));
                                        if let Some(age) = peer.status_age_secs {
                                            ui.small(format!("Status updated {}s ago", age));
                                        }
                                        ui.small(format!("Connected for {}s", peer.connected_secs));
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.button("Boot").clicked() {
                                                self.networking.disconnect_connection(&peer.connection_id);
                                            }
                                            if !peer.device_id.trim().is_empty() {
                                                if is_trusted {
                                                    if ui.button("Untrust").clicked() {
                                                        self.untrust_network_peer(
                                                            &peer.device_id,
                                                            &peer.device_name,
                                                        );
                                                    }
                                                } else if ui.button("Trust").clicked() {
                                                    self.trust_network_peer(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                            }
                                            if ui.button("Block").clicked()
                                                && !peer.device_id.trim().is_empty()
                                            {
                                                self.block_network_peer(
                                                    &peer.device_id,
                                                    &peer.device_name,
                                                );
                                            }
                                            if ui.button("Pass Stick").clicked() {
                                                self.networking_turn_holder = Some(key.clone());
                                                self.networking_shared_chat_policy.turn_mode =
                                                    SharedChatTurnMode::TalkingStick;
                                                self.networking_shared_chat_policy
                                                    .turn_holder_device_id = peer.device_id.clone();
                                                self.networking_shared_chat_policy
                                                    .turn_holder_device_name =
                                                    self.network_display_name(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                self.broadcast_shared_chat_policy(
                                                    "Talking stick reassigned from the device card.",
                                                );
                                            }
                                            if !peer.device_id.trim().is_empty()
                                                && ui.small_button("Copy ID").clicked()
                                            {
                                                ui.ctx().copy_text(peer.device_id.clone());
                                                self.networking_status = Some(format!(
                                                    "Copied device ID for {}.",
                                                    display_name
                                                ));
                                            }
                                            if ui.small_button("Copy info").clicked() {
                                                ui.ctx().copy_text(format!(
                                                    "Name: {} | Device ID: {} | Address: {} | Direction: {} | Connected: {}s",
                                                    display_name,
                                                    peer.device_id,
                                                    peer.address,
                                                    if peer.inbound {
                                                        "inbound"
                                                    } else {
                                                        "outbound"
                                                    },
                                                    peer.connected_secs
                                                ));
                                                self.networking_status = Some(format!(
                                                    "Copied connection info for {}.",
                                                    display_name
                                                ));
                                            }
                                        });
                                    });
                                    if card.response.hovered() {
                                        ui.painter().rect_stroke(
                                            card.response.rect.expand(1.0),
                                            6.0,
                                            egui::Stroke::new(
                                                if selected_initial { 1.9 } else { 1.35 },
                                                blend_color(
                                                    ui.visuals()
                                                        .widgets
                                                        .noninteractive
                                                        .bg_stroke
                                                        .color,
                                                    egui::Color32::from_rgb(70, 110, 180),
                                                    if selected_initial { 0.78 } else { 0.60 },
                                                ),
                                            ),
                                        );
                                    }
                                    ui.add_space(6.0);
                                }
                            });
                    }
                });

                ui.add_space(8.0);
                egui::CollapsingHeader::new(
                    RichText::new(format!("[TRU] Trusted ({})", trusted_visible.len()))
                        .color(egui::Color32::from_rgb(110, 80, 170))
                        .strong(),
                )
                .id_source("edu_network_trusted_section")
                .default_open(false)
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .id_source("edu_network_trusted_scroll")
                        .max_height(220.0)
                        .show(ui, |ui| {
                            if trusted_visible.is_empty() {
                                ui.label("(none)");
                            } else {
                                for peer in &trusted_visible {
                                    let display_name =
                                        self.network_display_name(&peer.device_id, &peer.device_name);
                                    let card = network_card_frame(
                                        ui,
                                        egui::Color32::from_rgb(235, 230, 247),
                                        egui::Color32::from_rgb(110, 80, 170),
                                        false,
                                    )
                                    .show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(RichText::new(display_name.clone()).strong());
                                            ui.small(
                                                RichText::new("Trusted")
                                                    .color(egui::Color32::from_rgb(110, 80, 170))
                                                    .strong(),
                                            );
                                            if let Some(group) =
                                                self.network_group_label(&peer.device_id)
                                            {
                                                ui.small(format!("Group: {group}"));
                                            }
                                        });
                                        let mut detail_parts = Vec::new();
                                        if !peer.address.trim().is_empty() {
                                            detail_parts.push(format!("Address: {}", peer.address));
                                        }
                                        if let Some(age) = peer.last_seen_secs_ago {
                                            detail_parts.push(format!("Last seen {}s ago", age));
                                        } else {
                                            detail_parts.push("Not seen recently".to_string());
                                        }
                                        ui.small(detail_parts.join(" | "));
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.button("Untrust").clicked() {
                                                self.untrust_network_peer(
                                                    &peer.device_id,
                                                    &peer.device_name,
                                                );
                                            }
                                            if ui.small_button("Copy ID").clicked() {
                                                ui.ctx().copy_text(peer.device_id.clone());
                                                self.networking_status = Some(format!(
                                                    "Copied device ID for {}.",
                                                    display_name
                                                ));
                                            }
                                            if ui.small_button("Copy info").clicked() {
                                                ui.ctx().copy_text(format!(
                                                    "Name: {} | Device ID: {} | Address: {} | State: trusted",
                                                    display_name, peer.device_id, peer.address
                                                ));
                                                self.networking_status = Some(format!(
                                                    "Copied connection info for {}.",
                                                    display_name
                                                ));
                                            }
                                        });
                                    });
                                    if card.response.hovered() {
                                        ui.painter().rect_stroke(
                                            card.response.rect.expand(1.0),
                                            6.0,
                                            egui::Stroke::new(
                                                1.35,
                                                blend_color(
                                                    ui.visuals()
                                                        .widgets
                                                        .noninteractive
                                                        .bg_stroke
                                                        .color,
                                                    egui::Color32::from_rgb(110, 80, 170),
                                                    0.60,
                                                ),
                                            ),
                                        );
                                    }
                                    ui.add_space(6.0);
                                }
                            }
                        });
                });

                ui.add_space(8.0);
                egui::CollapsingHeader::new(
                    RichText::new(format!("[BLK] Blocked ({})", blocked_visible.len()))
                        .color(egui::Color32::from_rgb(160, 60, 60))
                        .strong(),
                )
                    .id_source("edu_network_blocked_section")
                    .default_open(false)
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_source("edu_network_blocked_scroll")
                            .max_height(220.0)
                            .show(ui, |ui| {
                                if blocked_visible.is_empty() {
                                    ui.label("(none)");
                                } else {
                                    for peer in &blocked_visible {
                                        let key = peer.device_id.clone();
                                        let selected_initial =
                                            self.networking_selected_devices.contains(&key);
                                        let card = network_card_frame(
                                            ui,
                                            egui::Color32::from_rgb(248, 228, 228),
                                            egui::Color32::from_rgb(160, 60, 60),
                                            selected_initial,
                                        )
                                        .show(ui, |ui| {
                                            let display_name =
                                                self.network_display_name(&peer.device_id, &peer.device_name);
                                            let alias_editing = self
                                                .networking_alias_edit_device
                                                .as_deref()
                                                == Some(peer.device_id.as_str());
                                            let group_editing = self
                                                .networking_group_edit_device
                                                .as_deref()
                                                == Some(peer.device_id.as_str());

                                            ui.horizontal_wrapped(|ui| {
                                                let mut selected =
                                                    self.networking_selected_devices.contains(&key);
                                                if ui.checkbox(&mut selected, "").changed() {
                                                    if selected {
                                                        self.networking_selected_devices.insert(key.clone());
                                                    } else {
                                                        self.networking_selected_devices.remove(&key);
                                                    }
                                                }
                                                if ui
                                                    .link(RichText::new(display_name.clone()).strong())
                                                    .clicked()
                                                {
                                                    self.begin_network_alias_edit(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                                if let Some(group) =
                                                    self.network_group_label(&peer.device_id)
                                                {
                                                    if ui.small_button(format!("Group: {group}")).clicked()
                                                    {
                                                        self.begin_network_group_edit(&peer.device_id);
                                                    }
                                                } else if ui.small_button("+ Group").clicked() {
                                                    self.begin_network_group_edit(&peer.device_id);
                                                }
                                            });
                                            if alias_editing {
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.label("Rename:");
                                                    ui.add(
                                                        egui::TextEdit::singleline(
                                                            &mut self.networking_alias_input,
                                                        )
                                                        .desired_width(220.0)
                                                        .hint_text("Student desk 2"),
                                                    );
                                                    if ui.button("Save").clicked() {
                                                        self.save_network_alias_edit(
                                                            &peer.device_id,
                                                            &peer.device_name,
                                                        );
                                                    }
                                                    if ui.button("Cancel").clicked() {
                                                        self.cancel_network_alias_edit();
                                                    }
                                                });
                                            }
                                            if group_editing {
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.label("Group:");
                                                    ui.add(
                                                        egui::TextEdit::singleline(
                                                            &mut self.networking_group_input,
                                                        )
                                                        .desired_width(180.0)
                                                        .hint_text("e.g. Science Lab"),
                                                    );
                                                    if ui.button("Save group").clicked() {
                                                        self.save_network_group_edit(
                                                            &peer.device_id,
                                                            &peer.device_name,
                                                        );
                                                    }
                                                    if ui.button("Clear").clicked() {
                                                        self.networking_group_input.clear();
                                                        self.save_network_group_edit(
                                                            &peer.device_id,
                                                            &peer.device_name,
                                                        );
                                                    }
                                                    if ui.button("Cancel").clicked() {
                                                        self.cancel_network_group_edit();
                                                    }
                                                });
                                            }
                                            let mut status_line = vec!["Blocked".to_string()];
                                            if let Some(group) =
                                                self.network_group_label(&peer.device_id)
                                            {
                                                status_line.push(format!("Group: {group}"));
                                            }
                                            if let Some(age) = peer.last_seen_secs_ago {
                                                status_line.push(format!("Seen {age}s ago"));
                                            }
                                            ui.small(status_line.join(" | "));
                                            if !peer.address.trim().is_empty() {
                                                ui.small(format!("Address: {}", peer.address));
                                            }
                                            ui.horizontal_wrapped(|ui| {
                                                if ui.button("Unblock").clicked() {
                                                    self.unblock_network_peer(
                                                        &peer.device_id,
                                                        &peer.device_name,
                                                    );
                                                }
                                                if ui.small_button("Copy ID").clicked() {
                                                    ui.ctx().copy_text(peer.device_id.clone());
                                                    self.networking_status = Some(format!(
                                                        "Copied device ID for {}.",
                                                        display_name
                                                    ));
                                                }
                                                if ui.small_button("Connection info").clicked() {
                                                    ui.ctx().copy_text(format!(
                                                        "Name: {} | Device ID: {} | Address: {} | State: blocked",
                                                        display_name,
                                                        peer.device_id,
                                                        peer.address
                                                    ));
                                                    self.networking_status = Some(format!(
                                                        "Copied connection info for {}.",
                                                        display_name
                                                    ));
                                                }
                                            });
                                        });
                                        if card.response.hovered() {
                                            ui.painter().rect_stroke(
                                                card.response.rect.expand(1.0),
                                                6.0,
                                                egui::Stroke::new(
                                                    if selected_initial { 1.9 } else { 1.35 },
                                                    blend_color(
                                                        ui.visuals()
                                                            .widgets
                                                            .noninteractive
                                                            .bg_stroke
                                                            .color,
                                                        egui::Color32::from_rgb(160, 60, 60),
                                                        if selected_initial { 0.78 } else { 0.60 },
                                                    ),
                                                ),
                                            );
                                        }
                                        ui.add_space(6.0);
                                    }
                                }
                            });
                    });

                ui.add_space(12.0);
                ui.separator();
                let shared_room_highlight = highlight_active(NetworkingFocusSection::SharedRoom);
                let shared_room = egui::Frame::group(ui.style())
                    .fill(if shared_room_highlight {
                        egui::Color32::from_rgb(245, 246, 255)
                    } else {
                        ui.visuals().panel_fill
                    })
                    .stroke(if shared_room_highlight {
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 90, 170))
                    } else {
                        ui.visuals().widgets.noninteractive.bg_stroke
                    })
                    .show(ui, |ui| {
                        section_heading(
                            ui,
                            "[ROOM]",
                            egui::Color32::from_rgb(120, 90, 170),
                            "Shared room chat",
                        );
                        if shared_room_highlight {
                            ui.small(
                                RichText::new("Focused by Quick help")
                                    .strong()
                                    .color(egui::Color32::from_rgb(120, 90, 170)),
                            );
                        }
                        ui.label(
                            "Use this for classroom-wide shared chat. The main Chat tab can mirror into this room, while hot memory stays local and only deliberate luke warm summaries travel between devices.",
                        );

                        let capable_modules = self.shared_chat_capable_modules();
                        let mut next_turn_mode = self.networking_shared_chat_policy.turn_mode;
                        let mut next_ai_mode = self.networking_shared_chat_policy.ai_mode;
                        let mut teacher_override =
                            self.networking_shared_chat_policy.teacher_override;
                        let mut scope_selection = if self.networking_shared_chat_policy.scope_kind
                            == SharedChatScopeKind::Module
                        {
                            self.networking_shared_chat_policy.scope_module_id.clone()
                        } else {
                            "__general__".to_string()
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Scope:");
                            egui::ComboBox::from_id_source("edu_shared_room_scope")
                                .selected_text(self.shared_chat_scope_label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut scope_selection,
                                        "__general__".to_string(),
                                        "General room",
                                    );
                                    for (module_id, module_name, multiplayer) in &capable_modules {
                                        let label = if *multiplayer {
                                            format!("{module_name} (multiplayer)")
                                        } else {
                                            module_name.clone()
                                        };
                                        ui.selectable_value(
                                            &mut scope_selection,
                                            module_id.clone(),
                                            label,
                                        );
                                    }
                                });
                            ui.separator();
                            ui.label("Turn mode:");
                            egui::ComboBox::from_id_source("edu_shared_room_turn_mode")
                                .selected_text(next_turn_mode.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut next_turn_mode,
                                        SharedChatTurnMode::Open,
                                        "Open",
                                    );
                                    ui.selectable_value(
                                        &mut next_turn_mode,
                                        SharedChatTurnMode::TalkingStick,
                                        "Talking stick",
                                    );
                                });
                            ui.separator();
                            ui.label("AI mode:");
                            egui::ComboBox::from_id_source("edu_shared_room_ai_mode")
                                .selected_text(next_ai_mode.label())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut next_ai_mode,
                                        SharedChatAiMode::Off,
                                        "Off",
                                    );
                                    ui.selectable_value(
                                        &mut next_ai_mode,
                                        SharedChatAiMode::LocalAllowed,
                                        "Local allowed",
                                    );
                                    ui.selectable_value(
                                        &mut next_ai_mode,
                                        SharedChatAiMode::HostOnly,
                                        "Host only",
                                    );
                                });
                            ui.separator();
                            ui.add_enabled_ui(self.teacher_unlocked, |ui| {
                                ui.checkbox(&mut teacher_override, "Teacher override");
                            });
                        });
                        let current_scope_selection = if self.networking_shared_chat_policy.scope_kind
                            == SharedChatScopeKind::Module
                        {
                            self.networking_shared_chat_policy.scope_module_id.clone()
                        } else {
                            "__general__".to_string()
                        };
                        if scope_selection != current_scope_selection {
                            if scope_selection == "__general__" {
                                self.set_shared_chat_scope_general();
                            } else if let Some((_, module_name, multiplayer)) = capable_modules
                                .iter()
                                .find(|(module_id, _, _)| module_id == &scope_selection)
                            {
                                self.set_shared_chat_scope_module(
                                    scope_selection.clone(),
                                    module_name.clone(),
                                    *multiplayer,
                                );
                            }
                            self.broadcast_shared_chat_policy("Room scope changed.");
                        }
                        if next_turn_mode != self.networking_shared_chat_policy.turn_mode {
                            self.networking_shared_chat_policy.turn_mode = next_turn_mode;
                            if next_turn_mode == SharedChatTurnMode::Open {
                                self.networking_shared_chat_policy.turn_holder_device_id.clear();
                                self.networking_shared_chat_policy.turn_holder_device_name.clear();
                                self.networking_turn_holder = None;
                            }
                            self.broadcast_shared_chat_policy("Turn mode changed.");
                        }
                        if next_ai_mode != self.networking_shared_chat_policy.ai_mode {
                            self.networking_shared_chat_policy.ai_mode = next_ai_mode;
                            self.broadcast_shared_chat_policy("AI mode changed.");
                        }
                        if teacher_override != self.networking_shared_chat_policy.teacher_override {
                            self.networking_shared_chat_policy.teacher_override = teacher_override;
                            self.broadcast_shared_chat_policy("Teacher override changed.");
                        }

                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(
                                &mut self.networking_shared_chat_mirror_main_chat,
                                "Mirror local main chat into this room",
                            );
                            ui.small(format!(
                                "Host: {}",
                                if self
                                    .networking_shared_chat_policy
                                    .host_device_name
                                    .trim()
                                    .is_empty()
                                {
                                    "(not set)"
                                } else {
                                    self.networking_shared_chat_policy.host_device_name.trim()
                                }
                            ));
                            ui.small(format!(
                                "Turn holder: {}",
                                self.shared_chat_turn_holder_label()
                            ));
                            ui.small(format!(
                                "Connected peers in room: {}",
                                shared_room_connection_ids.len()
                            ));
                            if self.networking_shared_chat_policy.scope_kind
                                == SharedChatScopeKind::Module
                            {
                                ui.small(format!(
                                    "Scoped to module: {}",
                                    self.shared_chat_scope_label()
                                ));
                            }
                            if let Some(session_summary) = self.shared_chat_session_summary() {
                                ui.small(format!("Session: {session_summary}"));
                            }
                        });

                        if !self.networking_shared_chat_policy.session_active {
                            if let Some(recoverable) =
                                self.networking_recoverable_shared_chat_policy.clone()
                            {
                                ui.group(|ui| {
                                    ui.strong("Recovered host session available");
                                    ui.small(format!(
                                        "{} | scope {} | revision {}",
                                        if recoverable.session_label.trim().is_empty() {
                                            recoverable.session_id.trim()
                                        } else {
                                            recoverable.session_label.trim()
                                        },
                                        if recoverable.scope_kind == SharedChatScopeKind::Module
                                            && !recoverable.scope_module_name.trim().is_empty()
                                        {
                                            recoverable.scope_module_name.trim()
                                        } else {
                                            recoverable.label.trim()
                                        },
                                        recoverable.session_revision.max(1)
                                    ));
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.button("Resume saved session").clicked() {
                                            if let Err(err) =
                                                self.resume_recoverable_shared_chat_policy()
                                            {
                                                self.networking_status =
                                                    Some(format!("Networking: {err}"));
                                            }
                                        }
                                        if ui.button("Discard recovery").clicked() {
                                            self.discard_recoverable_shared_chat_policy();
                                            self.networking_status = Some(
                                                "Discarded the saved classroom host-session recovery snapshot."
                                                    .to_string(),
                                            );
                                        }
                                    });
                                });
                            }
                        } else if self.shared_chat_is_local_host()
                            && self.networking_recoverable_shared_chat_policy.is_some()
                        {
                            ui.small(
                                "Recovery snapshot armed: if this host restarts, you can resume this classroom room session cleanly.",
                            );
                        }

                        if let Some(recovery) = self.networking_recoverable_module_session.clone() {
                            ui.group(|ui| {
                                ui.strong("Recoverable module session state");
                                ui.small(format!(
                                    "{} | latest shared state: {} | cached assets: {}",
                                    if recovery.scope_module_name.trim().is_empty() {
                                        recovery.scope_module_id.trim()
                                    } else {
                                        recovery.scope_module_name.trim()
                                    },
                                    recovery
                                        .latest_shared_state
                                        .as_ref()
                                        .map(|state| format!("revision {}", state.session_revision.max(1)))
                                        .unwrap_or_else(|| "none yet".to_string()),
                                    recovery.recent_assets.len()
                                ));
                                ui.small(
                                    "Use this after a restart or host handoff to restore the module bridge locally, then re-share the last good session state or cached assets to selected devices (or the whole room if nothing is selected).",
                                );
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button("Restore state to bridge").clicked() {
                                        if let Err(err) =
                                            self.restore_recoverable_module_shared_state_to_bridge()
                                        {
                                            self.networking_status =
                                                Some(format!("Networking: {err}"));
                                        }
                                    }
                                    if ui.button("Re-share latest state").clicked() {
                                        if let Err(err) = self.replay_recoverable_module_shared_state()
                                        {
                                            self.networking_status =
                                                Some(format!("Networking: {err}"));
                                        }
                                    }
                                    if ui
                                        .add_enabled(
                                            !recovery.recent_assets.is_empty(),
                                            egui::Button::new("Replay cached assets"),
                                        )
                                        .clicked()
                                    {
                                        if let Err(err) = self.replay_recoverable_module_assets() {
                                            self.networking_status =
                                                Some(format!("Networking: {err}"));
                                        }
                                    }
                                    if ui.button("Open recovery folder").clicked() {
                                        open_path_in_explorer(&self.network_recovery_dir());
                                    }
                                });
                            });
                        }

                        if self.networking_shared_chat_policy.session_active
                            && self.shared_chat_host_appears_offline()
                        {
                            ui.group(|ui| {
                                ui.colored_label(
                                    egui::Color32::from_rgb(180, 110, 70),
                                    "Current room host appears offline.",
                                );
                                ui.small(
                                    "You can wait for the host to return, or take over and rebroadcast this classroom room from here.",
                                );
                                if ui.button("Take over as host").clicked() {
                                    if let Err(err) = self.take_over_shared_chat_host() {
                                        self.networking_status =
                                            Some(format!("Networking: {err}"));
                                    }
                                }
                            });
                        }

                        let selected_connected_peers = snapshot
                            .connected_peers
                            .iter()
                            .filter(|peer| {
                                let key = if peer.device_id.trim().is_empty() {
                                    peer.connection_id.clone()
                                } else {
                                    peer.device_id.clone()
                                };
                                self.networking_selected_devices.contains(&key)
                            })
                            .collect::<Vec<_>>();
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Broadcast current room policy").clicked() {
                                self.broadcast_shared_chat_policy("Manual policy refresh.");
                            }
                            if self.networking_shared_chat_policy.scope_kind
                                == SharedChatScopeKind::Module
                            {
                                if !self.networking_shared_chat_policy.session_active {
                                    if ui.button("Start module session").clicked() {
                                        if let Some(module_name) =
                                            self.begin_shared_chat_module_session()
                                        {
                                            self.broadcast_shared_chat_policy(&format!(
                                                "Started classroom module session for {module_name}."
                                            ));
                                        }
                                    }
                                } else if ui.button("End module session").clicked() {
                                    let label = self
                                        .networking_shared_chat_policy
                                        .session_label
                                        .trim()
                                        .to_string();
                                    self.end_shared_chat_module_session();
                                    self.broadcast_shared_chat_policy(&format!(
                                        "Ended {}.",
                                        if label.is_empty() {
                                            "the module session".to_string()
                                        } else {
                                            label
                                        }
                                    ));
                                }
                            }
                            if ui.button("Take stick").clicked() {
                                let local = self.networking.snapshot().clone();
                                self.networking_shared_chat_policy.turn_mode =
                                    SharedChatTurnMode::TalkingStick;
                                self.networking_shared_chat_policy.turn_holder_device_id =
                                    local.device_id.clone();
                                self.networking_shared_chat_policy.turn_holder_device_name =
                                    local.device_name.clone();
                                self.networking_turn_holder = Some(local.device_id);
                                self.broadcast_shared_chat_policy("Teacher took the talking stick.");
                            }
                            if ui
                                .add_enabled(
                                    self.shared_chat_is_local_host()
                                        && selected_connected_peers.len() == 1,
                                    egui::Button::new("Hand off host to selected peer"),
                                )
                                .clicked()
                            {
                                if let Some(peer) = selected_connected_peers.first() {
                                    if let Err(err) = self.handoff_shared_chat_host_to_peer(
                                        &peer.device_id,
                                        &self.network_display_name(
                                            &peer.device_id,
                                            &peer.device_name,
                                        ),
                                    ) {
                                        self.networking_status =
                                            Some(format!("Networking: {err}"));
                                    }
                                }
                            }
                            if ui
                                .add_enabled(
                                    selected_connected_peers.len() == 1,
                                    egui::Button::new("Pass stick to selected peer"),
                                )
                                .clicked()
                            {
                                if let Some(peer) = selected_connected_peers.first() {
                                    self.networking_shared_chat_policy.turn_mode =
                                        SharedChatTurnMode::TalkingStick;
                                    self.networking_shared_chat_policy.turn_holder_device_id =
                                        peer.device_id.clone();
                                    self.networking_shared_chat_policy.turn_holder_device_name =
                                        self.network_display_name(&peer.device_id, &peer.device_name);
                                    self.networking_turn_holder = Some(peer.device_id.clone());
                                    self.broadcast_shared_chat_policy(
                                        "Talking stick reassigned.",
                                    );
                                }
                            }
                            if ui.button("Open room flow").clicked() {
                                self.networking_shared_chat_policy.turn_mode =
                                    SharedChatTurnMode::Open;
                                self.networking_shared_chat_policy.turn_holder_device_id.clear();
                                self.networking_shared_chat_policy.turn_holder_device_name.clear();
                                self.networking_turn_holder = None;
                                self.broadcast_shared_chat_policy("Talking stick cleared.");
                            }
                        });

                        let room_hint = if shared_room_connection_ids.is_empty() {
                            "Connect to one or more peers to turn the shared room into a live classroom conversation."
                                .to_string()
                        } else {
                            match self.shared_chat_can_send_user_message() {
                                Ok(()) => "You can type here to send a room message, or mirror the main Chat tab into this room.".to_string(),
                                Err(reason) => reason,
                            }
                        };
                        ui.small(room_hint);

                        ScrollArea::vertical()
                            .id_source("edu_shared_room_log")
                            .max_height(200.0)
                            .show(ui, |ui| {
                                if self.networking_shared_chat_log.is_empty() {
                                    ui.label("(no shared room activity yet)");
                                } else {
                                    for entry in self
                                        .networking_shared_chat_log
                                        .iter()
                                        .rev()
                                        .take(48)
                                        .rev()
                                    {
                                        ui.group(|ui| {
                                            ui.horizontal_wrapped(|ui| {
                                                let tag_color = match entry.speaker_kind.as_str() {
                                                    "assistant" => {
                                                        egui::Color32::from_rgb(50, 140, 90)
                                                    }
                                                    "system" => {
                                                        egui::Color32::from_rgb(120, 90, 170)
                                                    }
                                                    _ => egui::Color32::from_rgb(70, 110, 180),
                                                };
                                                ui.label(
                                                    RichText::new(format!(
                                                        "[{}]",
                                                        entry.speaker_kind.to_uppercase()
                                                    ))
                                                    .color(tag_color)
                                                    .strong(),
                                                );
                                                ui.strong(if entry.speaker_label.trim().is_empty() {
                                                    entry.from_device_name.trim()
                                                } else {
                                                    entry.speaker_label.trim()
                                                });
                                                ui.small(entry.from_device_name.clone());
                                                if entry.scope_kind == SharedChatScopeKind::Module {
                                                    let scope_label =
                                                        if entry.scope_module_name.trim().is_empty() {
                                                            entry.scope_module_id.trim()
                                                        } else {
                                                            entry.scope_module_name.trim()
                                                        };
                                                    if !scope_label.is_empty() {
                                                        ui.small(format!("scope: {scope_label}"));
                                                    }
                                                }
                                            });
                                            ui.label(entry.body.trim());
                                        });
                                        ui.add_space(4.0);
                                    }
                                }
                            });

                        ui.horizontal(|ui| {
                            let input = ui.add(
                                egui::TextEdit::singleline(&mut self.networking_shared_chat_input)
                                    .desired_width(ui.available_width() - 120.0)
                                    .hint_text("Shared room message..."),
                            );
                            let send_enabled = !self.networking_shared_chat_input.trim().is_empty()
                                && !shared_room_connection_ids.is_empty()
                                && self.shared_chat_can_send_user_message().is_ok();
                            if input.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                && send_enabled
                            {
                                let body = self.networking_shared_chat_input.trim().to_string();
                                self.networking_shared_chat_input.clear();
                                self.broadcast_shared_chat_message("user", "You", &body);
                            }
                            if ui
                                .add_enabled(send_enabled, egui::Button::new("Send to room"))
                                .clicked()
                            {
                                let body = self.networking_shared_chat_input.trim().to_string();
                                self.networking_shared_chat_input.clear();
                                self.broadcast_shared_chat_message("user", "You", &body);
                            }
                        });
                    });
                if pending_focus == Some(NetworkingFocusSection::SharedRoom) {
                    shared_room.response.scroll_to_me(Some(Align::Center));
                }

                ui.add_space(12.0);
                ui.separator();
                section_heading(
                    ui,
                    "[EVT]",
                    egui::Color32::from_rgb(170, 110, 70),
                    "Recent session events",
                );
                ui.label(
                    "Low-latency room events are meant for lightweight classroom/module signals like ready states, turn nudges, small game moves, or other fast session updates.",
                );
                ui.horizontal_wrapped(|ui| {
                    ui.small(format!(
                        "Recent events cached: {}",
                        snapshot.received_session_events.len()
                    ));
                    if !snapshot.received_session_events.is_empty()
                        && ui.button("Clear recent events").clicked()
                    {
                        self.networking.clear_received_session_events();
                    }
                });
                if snapshot.received_session_events.is_empty() {
                    ui.label("(no recent session events yet)");
                } else {
                    for event in snapshot.received_session_events.iter().rev().take(24) {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(if event.label.trim().is_empty() {
                                    event.event_type.trim()
                                } else {
                                    event.label.trim()
                                });
                                ui.small(format!(
                                    "{} | {}s ago",
                                    if event.from_device_name.trim().is_empty() {
                                        "(unknown sender)"
                                    } else {
                                        event.from_device_name.trim()
                                    },
                                    event.received_secs_ago
                                ));
                                if !event.scope_module_id.trim().is_empty() {
                                    ui.small(format!("module: {}", event.scope_module_id.trim()));
                                }
                                if !event.session_id.trim().is_empty() {
                                    ui.small(format!("session: {}", event.session_id.trim()));
                                }
                                if !event.from_address.trim().is_empty() {
                                    ui.small(format!("addr: {}", event.from_address.trim()));
                                }
                                if !event.content_type.trim().is_empty() {
                                    ui.small(event.content_type.trim());
                                }
                            });
                            if !event.payload_text.trim().is_empty() {
                                ui.label(event.payload_text.trim());
                            } else {
                                ui.small("(no text payload)");
                            }
                        });
                        ui.add_space(4.0);
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                section_heading(
                    ui,
                    "[OUT]",
                    egui::Color32::from_rgb(120, 90, 170),
                    "Cross-instance handoff",
                );
                ui.label(
                    "Pass a concise brief to another connected Chatty-EDU instance without leaving the local network.",
                );

                if snapshot.connected_peers.is_empty() {
                    ui.label("Connect to another Chatty-EDU instance to send a handoff.");
                } else {
                    let selected_label = snapshot
                        .connected_peers
                        .iter()
                        .find(|peer| peer.connection_id == self.networking_handoff_target)
                        .map(|peer| peer.device_name.clone())
                        .unwrap_or_else(|| "(choose target)".to_string());

                    egui::ComboBox::from_id_source("edu_network_handoff_target")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            for peer in &snapshot.connected_peers {
                                ui.selectable_value(
                                    &mut self.networking_handoff_target,
                                    peer.connection_id.clone(),
                                    peer.device_name.clone(),
                                );
                            }
                        });

                    ui.add(
                        egui::TextEdit::singleline(&mut self.networking_handoff_title)
                            .hint_text("Short handoff title..."),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut self.networking_handoff_body)
                            .desired_rows(5)
                            .hint_text("What should the other EDU instance know or pick up?"),
                    );

                    let send_enabled = !self.networking_handoff_target.trim().is_empty()
                        && !self.networking_handoff_body.trim().is_empty();
                    if ui
                        .add_enabled(send_enabled, egui::Button::new("Send handoff"))
                        .clicked()
                    {
                        self.networking.send_handoff(
                            &self.networking_handoff_target,
                            &self.networking_handoff_title,
                            &self.networking_handoff_body,
                        );
                        self.networking_handoff_title.clear();
                        self.networking_handoff_body.clear();
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                section_heading(
                    ui,
                    "[ACK]",
                    egui::Color32::from_rgb(80, 120, 170),
                    "Recent delivery status",
                );
                if delivery_visible.is_empty() {
                ui.label("(no recent outgoing transfers yet)");
                } else {
                    for artifact in &delivery_visible {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(if artifact.label.trim().is_empty() {
                                    artifact.kind.trim()
                                } else {
                                    artifact.label.trim()
                                });
                                ui.small(format!(
                                    "{} | {} attempt(s) | {}s ago",
                                    artifact.status.trim(),
                                    artifact.attempts,
                                    artifact.updated_secs_ago
                                ));
                                ui.small(if artifact.waiting_for_ack {
                                    "Awaiting ack"
                                } else {
                                    "Closed loop"
                                });
                            });
                            ui.monospace(&artifact.artifact_id);
                            if !artifact.to_device_name.trim().is_empty() {
                                ui.small(format!("To: {}", artifact.to_device_name));
                            }
                            if !artifact.to_device_id.trim().is_empty() {
                                ui.monospace(&artifact.to_device_id);
                            }
                            if !artifact.to_address.trim().is_empty() {
                                ui.small(format!("Address: {}", artifact.to_address));
                            }
                            if !artifact.module_id.trim().is_empty() {
                                ui.small(format!("Module: {}", artifact.module_id));
                            }
                            if !artifact.file_name.trim().is_empty() {
                                ui.small(format!("File: {}", artifact.file_name));
                            }
                            ui.small(format_network_transfer_meta(
                                &artifact.content_type,
                                &artifact.transfer_encoding,
                                artifact.byte_len,
                                artifact.chunk_count,
                            ));
                            if !artifact.summary.trim().is_empty() {
                                ui.label(artifact.summary.trim());
                            }
                        });
                        ui.add_space(6.0);
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("[IN]")
                            .color(egui::Color32::from_rgb(120, 90, 170))
                            .strong(),
                    );
                    ui.label(RichText::new("Received handoffs").strong());
                    if !snapshot.received_handoffs.is_empty() && ui.button("Clear received").clicked()
                    {
                        self.networking.clear_received_handoffs();
                        self.networking_seen_handoffs.clear();
                    }
                });

                if snapshot.received_handoffs.is_empty() {
                    ui.label("(none yet)");
                } else {
                    for handoff in &snapshot.received_handoffs {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(if handoff.title.trim().is_empty() {
                                    "(untitled handoff)"
                                } else {
                                    handoff.title.trim()
                                });
                                ui.small(format!(
                                    "from {} | {}s ago",
                                    handoff.from_device_name, handoff.received_secs_ago
                                ));
                            });
                            ui.monospace(&handoff.from_device_id);
                            ui.small(format!("Address: {}", handoff.from_address));
                            ui.label(&handoff.body);
                        });
                        ui.add_space(6.0);
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("[XFER]")
                            .color(egui::Color32::from_rgb(70, 140, 90))
                            .strong(),
                    );
                    ui.label(RichText::new("Received transfers").strong());
                    if !received_transfer_visible.is_empty()
                        && ui.button("Clear transfers").clicked()
                    {
                        self.networking.clear_received_artifacts();
                        self.networking_seen_artifacts.clear();
                    }
                });

                if received_transfer_visible.is_empty() {
                    ui.label("(no packs, setup bundles, or shared module states received yet)");
                } else {
                    for artifact in &received_transfer_visible {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(if artifact.label.trim().is_empty() {
                                    "(untitled transfer)"
                                } else {
                                    artifact.label.trim()
                                });
                                ui.small(format!(
                                    "{} from {} | {}s ago",
                                    artifact.kind.trim(),
                                    artifact.from_device_name,
                                    artifact.received_secs_ago
                                ));
                            });
                            ui.monospace(&artifact.from_device_id);
                            if !artifact.module_id.trim().is_empty() {
                                ui.small(format!("Module: {}", artifact.module_id));
                            }
                            if !artifact.file_name.trim().is_empty() {
                                ui.small(format!("File: {}", artifact.file_name));
                            }
                            ui.small(format_network_transfer_meta(
                                &artifact.content_type,
                                &artifact.transfer_encoding,
                                artifact.byte_len,
                                artifact.chunk_count,
                            ));
                            if artifact.is_binary() {
                                ui.small("Payload: binary/file-style transfer");
                            }
                            if !artifact.summary.trim().is_empty() {
                                ui.label(artifact.summary.trim());
                            }
                            ui.small(format!("Address: {}", artifact.from_address));
                        });
                        ui.add_space(6.0);
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                section_heading(
                    ui,
                    "[FIL]",
                    egui::Color32::from_rgb(110, 130, 80),
                    "Received file-style transfers",
                );
                self.render_received_generic_transfer_inbox(ui, "Received file transfers");

                ui.add_space(12.0);
                ui.separator();
                section_heading(
                    ui,
                    "[SET]",
                    egui::Color32::from_rgb(90, 110, 170),
                    "Received classroom setup bundles",
                );
                self.render_received_workflow_bundle_inbox(
                    ui,
                    "Received classroom setup bundles",
                );

                ui.add_space(12.0);
                ui.separator();
                section_heading(
                    ui,
                    "[MEM]",
                    egui::Color32::from_rgb(130, 90, 170),
                    "Received luke warm context",
                );
                self.render_received_lukewarm_inbox(ui, "Received luke warm context");
            });
    }

    fn render_chatty_thoughts_panel(&self, ui: &mut egui::Ui) {
        let thoughts = self.chatty_thoughts_display_items();
        self.render_memory_sidebar(
            ui,
            "Chatty's thoughts",
            "This is what Chatty can currently remember in this session. It clears when the app closes.",
            &thoughts,
            "Nothing has built up in this session yet.",
            Some(180),
        );
    }

    fn render_memory_jogger_panel(&self, ui: &mut egui::Ui) {
        let items = self.memory_jogger_items();
        self.render_memory_sidebar(
            ui,
            "Memory jogger",
            "Helps Chatty remember what you've been working on recently across sessions.",
            &items,
            "No recent memory jogger notes yet. It updates when the app closes.",
            Some(180),
        );
    }

    fn render_memory_sidebar(
        &self,
        ui: &mut egui::Ui,
        title: &str,
        tooltip: &str,
        items: &[String],
        empty_message: &str,
        display_clip_chars: Option<usize>,
    ) {
        ui.push_id(("memory_sidebar", title), |ui| {
            egui::Frame::none()
                .fill(color_from_hex(&self.theme.surface))
                .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
                .rounding(egui::Rounding::same(6.0))
                .inner_margin(egui::vec2(10.0, 8.0))
                .show(ui, |ui| {
                    let heading = ui.label(RichText::new(title).strong());
                    let _ = heading.on_hover_text(tooltip);
                    ui.add_space(6.0);

                    ScrollArea::vertical()
                        .id_source(("memory_sidebar_scroll", title))
                        .auto_shrink([false; 2])
                        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                        .show(ui, |ui| {
                            if items.is_empty() {
                                ui.label(
                                    RichText::new(empty_message)
                                        .small()
                                        .color(color_from_hex(&self.theme.muted_text)),
                                );
                                return;
                            }

                            for item in items {
                                let full_text = Self::display_safe_text(item);
                                let display_text = if let Some(max_chars) = display_clip_chars {
                                    Self::clip_chars(&full_text, max_chars)
                                } else {
                                    full_text.clone()
                                };
                                let response = ui.add_sized(
                                    [ui.available_width(), 0.0],
                                    egui::Label::new(
                                        RichText::new(format!("- {display_text}"))
                                            .small()
                                            .color(color_from_hex(&self.theme.text)),
                                    )
                                    .wrap(true),
                                );
                                if display_clip_chars.is_some() {
                                    let _ = response.on_hover_text(full_text);
                                }
                                ui.add_space(6.0);
                            }
                        });
                });
        });
    }

    fn render_task_ledger_snapshot(&self, ui: &mut egui::Ui, summary: Option<&TaskLedgerSummary>) {
        if let Some(summary) = summary {
            ui.small(
                "Read-only summary of the structured task ledger. Use the ledger itself for edits.",
            );
            ui.add_space(4.0);
            ui.label(format!(
                "Current task: {}",
                if summary.current_task.trim().is_empty() {
                    "(not set)"
                } else {
                    summary.current_task.trim()
                }
            ));
            ui.label(format!(
                "Next step: {}",
                if summary.next_step.trim().is_empty() {
                    "(not set)"
                } else {
                    summary.next_step.trim()
                }
            ));
            ui.horizontal_wrapped(|ui| {
                ui.small(format!("Open questions: {}", summary.open_questions.len()));
                ui.small(format!("Files touched: {}", summary.files_touched.len()));
                ui.small(format!("Working notes: {}", summary.notes.len()));
            });

            if !summary.open_questions.is_empty() {
                ui.add_space(4.0);
                ui.small("Open questions:");
                for item in summary.open_questions.iter().take(3) {
                    ui.small(format!("- {}", truncate_for_ui(item, 120)));
                }
            }

            if !summary.files_touched.is_empty() {
                ui.add_space(4.0);
                ui.small(format!(
                    "Recent files: {}",
                    truncate_for_ui(&summary.files_touched.join(", "), 180)
                ));
            }
        } else {
            ui.small("Task ledger not available yet.");
        }
    }

    fn chatty_thoughts_display_items(&self) -> Vec<String> {
        self.chatty_thoughts_items(None)
    }

    fn chatty_thoughts_prompt_block(&self) -> Option<String> {
        let mut lines = Vec::new();

        if let Some(assignment) = self.selected_assignment_ref() {
            lines.push(format!(
                "Current homework: {} ({}) | {} | Year {}",
                assignment.title, assignment.id, assignment.subject, assignment.year_level
            ));
        }

        let pairs = self.recent_chat_exchange_pair_entries(Some(320), 6);
        if !pairs.is_empty() {
            lines.push("Recent message pairs still in context:".to_string());
            for (index, (user_msg, chatty_msg)) in pairs.into_iter().enumerate() {
                let number = index + 1;
                match (user_msg, chatty_msg) {
                    (Some(user_msg), Some(chatty_msg)) => {
                        lines.push(format!("{number}. You: {user_msg}"));
                        lines.push(format!("   Chatty: {chatty_msg}"));
                    }
                    (Some(user_msg), None) => {
                        lines.push(format!("{number}. You: {user_msg}"));
                    }
                    (None, Some(chatty_msg)) => {
                        lines.push(format!("{number}. Chatty: {chatty_msg}"));
                    }
                    (None, None) => {}
                }
            }
        }

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    fn chatty_thoughts_items(&self, clip_chars: Option<usize>) -> Vec<String> {
        let mut thoughts = Vec::new();

        if let Some(assignment) = self.selected_assignment_ref() {
            thoughts.push(format!(
                "Current homework: {} ({}) | {} | Year {}",
                assignment.title, assignment.id, assignment.subject, assignment.year_level
            ));
        }

        for pair in self.recent_chat_exchange_pairs(clip_chars, 4) {
            thoughts.push(pair);
        }

        thoughts.truncate(5);
        thoughts
    }

    fn recent_chat_exchange_pair_entries(
        &self,
        clip_chars: Option<usize>,
        max_pairs: usize,
    ) -> Vec<(Option<String>, Option<String>)> {
        let recent_messages = self
            .chat_log
            .iter()
            .filter(|(_, message)| {
                let trimmed = message.trim();
                !trimmed.is_empty() && trimmed != "..."
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut pairs = Vec::new();
        let mut pending_user: Option<String> = None;

        for (sender, message) in recent_messages {
            let prepared = Self::prepare_memory_text(&message, clip_chars);
            if prepared.is_empty() {
                continue;
            }

            if sender.eq_ignore_ascii_case("you") {
                if let Some(previous_user) = pending_user.replace(prepared) {
                    pairs.push((Some(previous_user), None));
                }
            } else if sender.eq_ignore_ascii_case("chatty") {
                pairs.push((pending_user.take(), Some(prepared)));
            }
        }

        if let Some(user_msg) = pending_user.take() {
            pairs.push((Some(user_msg), None));
        }

        if pairs.len() > max_pairs {
            pairs = pairs[pairs.len().saturating_sub(max_pairs)..].to_vec();
        }

        pairs
    }

    fn recent_chat_exchange_pairs(
        &self,
        clip_chars: Option<usize>,
        max_pairs: usize,
    ) -> Vec<String> {
        self.recent_chat_exchange_pair_entries(clip_chars, max_pairs)
            .into_iter()
            .filter_map(|(user_msg, chatty_msg)| match (user_msg, chatty_msg) {
                (Some(user_msg), Some(chatty_msg)) => Some(format!(
                    "Recent exchange: You said \"{user_msg}\". Chatty replied \"{chatty_msg}\"."
                )),
                (Some(user_msg), None) => {
                    Some(format!("Recent exchange: You said \"{user_msg}\"."))
                }
                (None, Some(chatty_msg)) => {
                    Some(format!("Recent exchange: Chatty replied \"{chatty_msg}\"."))
                }
                (None, None) => None,
            })
            .collect()
    }

    fn memory_jogger_items(&self) -> Vec<String> {
        self.memory_jogger
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| line.trim_start_matches("- ").trim().to_string())
            .collect()
    }

    fn render_chat_context_banner(&self, ui: &mut egui::Ui) {
        if let Some(assignment) = self.selected_assignment_ref() {
            egui::Frame::none()
                .fill(color_from_hex(&self.theme.surface))
                .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
                .rounding(egui::Rounding::same(6.0))
                .inner_margin(egui::vec2(10.0, 8.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Homework context active").strong());
                    ui.label(format!(
                        "{} ({}) | {} | Year {}",
                        assignment.title, assignment.id, assignment.subject, assignment.year_level
                    ));
                    if self.settings.homework_hints_only {
                        ui.label(
                            "Questions about this assignment will get hints, not full solutions.",
                        );
                    } else {
                        ui.label(
                            "If you ask about this assignment here, Chatty will use that homework context.",
                        );
                    }
                });
            ui.add_space(8.0);
        }
    }

    fn render_chat_log(&self, ui: &mut egui::Ui, log_height: f32) {
        ScrollArea::vertical()
            .id_source("main_chat_log_scroll")
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .max_height(log_height)
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                ui.set_min_height(log_height);
                let max_width = ui.available_width() * 0.96;
                ui.set_max_width(max_width);
                self.render_chat_messages(ui, &self.chat_log, max_width);
                ui.add_space(12.0);
            });
    }

    fn render_home_chat_preview(&self, ui: &mut egui::Ui) {
        let preview_start = self.chat_log.len().saturating_sub(4);
        let preview_messages = &self.chat_log[preview_start..];

        egui::Frame::none()
            .fill(color_from_hex(&self.theme.surface))
            .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::vec2(10.0, 8.0))
            .show(ui, |ui| {
                ui.set_min_height(148.0);
                if !self.chat_log.is_empty() && self.chat_log.len() > preview_messages.len() {
                    ui.label(
                        RichText::new("Showing the latest chat messages. Open Chat for the full conversation.")
                            .small()
                            .color(color_from_hex(&self.theme.muted_text)),
                    );
                    ui.add_space(4.0);
                }

                let max_width = ui.available_width() * 0.98;
                ui.set_max_width(max_width);
                self.render_chat_messages(ui, preview_messages, max_width);
            });
    }

    fn render_chat_messages(
        &self,
        ui: &mut egui::Ui,
        messages: &[(String, String)],
        max_width: f32,
    ) {
        if messages.is_empty() {
            ui.add_space(8.0);
            ui.label("No chat messages yet. Use the bottom chat bar to start a conversation.");
            return;
        }

        for (sender, msg) in messages {
            let is_user = sender.eq_ignore_ascii_case("you");
            let bubble_fill = if is_user {
                color_from_hex(&self.theme.accent_soft)
            } else {
                color_from_hex(&self.theme.surface)
            };
            let bubble_stroke = if is_user {
                color_from_hex(&self.theme.accent)
            } else {
                color_from_hex(&self.theme.border)
            };
            let text_color = if is_user {
                color_from_hex(&self.theme.accent)
            } else {
                color_from_hex(&self.theme.text)
            };
            let name_color = if is_user {
                bubble_stroke
            } else {
                color_from_hex(&self.theme.muted_text)
            };

            ui.add_space(4.0);
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.add_space(8.0);
                egui::Frame::none()
                    .fill(bubble_fill)
                    .stroke(egui::Stroke {
                        width: 1.0,
                        color: bubble_stroke,
                    })
                    .rounding(egui::Rounding::same(6.0))
                    .inner_margin(egui::vec2(10.0, 8.0))
                    .show(ui, |ui| {
                        ui.set_max_width(max_width * 0.9);
                        let safe_sender = Self::display_safe_text(sender);
                        let safe_message = Self::display_safe_text(msg);
                        ui.label(RichText::new(safe_sender).strong().color(name_color));
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(RichText::new(safe_message).color(text_color))
                                .wrap(true),
                        );
                    });
            });
        }
    }

    fn render_bookkeeper(&mut self, ui: &mut egui::Ui) {
        if !self.teacher_unlocked {
            ui.heading("Bookkeeper");
            ui.label("Unlock teacher view to open full session logs.");
            return;
        }

        let heading = ui.label(RichText::new("Bookkeeper logs").heading());
        let _ =
            heading.on_hover_text("Full session logs. Search past activity and diagnose issues.");
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Search");
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.bookkeeper_query)
                    .hint_text("Search chat text, dates, or homework labels"),
            );
            if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.refresh_bookkeeper_search();
            }
            if ui.button("Search logs").clicked() {
                self.refresh_bookkeeper_search();
            }
            if ui.button("Show recent").clicked() {
                self.bookkeeper_query.clear();
                self.refresh_bookkeeper_search();
            }
        });

        if let Some(status) = &self.bookkeeper_status {
            ui.add_space(4.0);
            ui.label(
                RichText::new(status)
                    .small()
                    .color(color_from_hex(&self.theme.muted_text)),
            );
        }
        ui.add_space(8.0);

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(false)
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                if self.bookkeeper_results.is_empty() {
                    ui.label("No cold log entries matched this search yet.");
                    return;
                }

                for entry in &self.bookkeeper_results {
                    egui::Frame::none()
                        .fill(color_from_hex(&self.theme.surface))
                        .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::vec2(10.0, 8.0))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(&entry.timestamp)
                                        .small()
                                        .color(color_from_hex(&self.theme.muted_text)),
                                );
                                ui.label(
                                    RichText::new(format!("{} | {}", entry.channel, entry.speaker))
                                        .small()
                                        .strong(),
                                );
                                ui.label(
                                    RichText::new(format!("session {}", entry.session_id))
                                        .small()
                                        .color(color_from_hex(&self.theme.muted_text)),
                                );
                            });
                            if let Some(note) = &entry.note {
                                if !note.trim().is_empty() {
                                    ui.label(
                                        RichText::new(note)
                                            .small()
                                            .color(color_from_hex(&self.theme.accent)),
                                    );
                                }
                            }
                            ui.add_space(4.0);
                            ui.label(entry.text.trim());
                        });
                    ui.add_space(8.0);
                }
            });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.label("Student profile");
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.settings.student.student_name);
        });
        ui.horizontal(|ui| {
            ui.label("Student ID");
            ui.text_edit_singleline(&mut self.settings.student.student_id);
        });
        ui.horizontal(|ui| {
            ui.label("Class ID");
            ui.text_edit_singleline(&mut self.settings.student.class_id);
        });
        ui.separator();
        ui.label(RichText::new("Model").strong());
        ui.horizontal(|ui| {
            ui.label("Max tokens");
            ui.add_enabled(
                self.teacher_unlocked,
                egui::DragValue::new(&mut self.settings.model.max_tokens).clamp_range(32..=4096),
            );
            ui.label("Higher = longer answers (slower).");
        });
        if !self.teacher_unlocked {
            ui.colored_label(
                self.warning_color(),
                "Unlock teacher view to change model limits.",
            );
        }
        ui.separator();
        if !self.teacher_unlocked {
            ui.label(RichText::new("Teacher controls").strong());
            ui.colored_label(
                self.warning_color(),
                "Teacher controls are locked. Unlock via the Teacher menu.",
            );
        } else {
            ui.label(RichText::new("Teacher controls").strong());
            ui.checkbox(
                &mut self.settings.janet.enabled,
                "Enable Janet safety filter",
            );
            ui.checkbox(
                &mut self.settings.janet.block_swears,
                "Block swears and rude words",
            );
            ui.checkbox(
                &mut self.settings.janet.block_mature_topics,
                "Block mature topics",
            );
            ui.separator();
            ui.checkbox(
                &mut self.settings.homework_hints_only,
                "Homework help gives hints only (no full answers)",
            );
            ui.separator();
            ui.checkbox(&mut self.settings.game.enabled, "Enable games");
            ui.checkbox(
                &mut self.settings.game.games_in_class_allowed,
                "Allow games in class",
            );
            ui.separator();
            ui.checkbox(
                &mut self.settings.allow_sandbox_tool_requests,
                "Allow sandbox tool requests",
            );
            ui.small(
                "Keeps Chatty-EDU's file work inside Chatty_Sandbox/ and still requires approval before actions run.",
            );
        }
        if ui.button("Save settings").clicked() {
            let _ = save_settings(&self.settings, &self.base_path);
            ui.label("Saved");
        }
    }

    fn render_diagnostics(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        let desired = if self.teacher_unlocked {
            DiagnosticsAudience::Teacher
        } else {
            DiagnosticsAudience::StudentSafe
        };
        if self.diagnostics_report.trim().is_empty() || self.diagnostics_audience != desired {
            self.refresh_diagnostics_report();
        }

        ui.heading("Diagnostic / Health Check");
        ui.label("Copy/paste this report when asking for support.");
        ui.label(format!(
            "Report type: {}",
            if self.teacher_unlocked {
                "teacher"
            } else {
                "student-safe"
            }
        ));

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.refresh_diagnostics_report();
            }
            if ui.button("Copy report").clicked() {
                if self.diagnostics_report.trim().is_empty() {
                    self.refresh_diagnostics_report();
                }
                ctx.copy_text(self.diagnostics_report.clone());
                self.diagnostics_status = Some("Copied report to clipboard.".to_string());
            }
            if let Some(msg) = &self.diagnostics_status {
                ui.colored_label(self.warning_color(), msg);
            }
        });

        ui.separator();
        let desired_rows = self.diagnostics_report.lines().count().max(24).min(4000);
        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .max_height(ui.available_height())
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.diagnostics_report)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(desired_rows)
                        .interactive(false),
                );
            });
    }

    fn normalize_model_message(text: &str) -> String {
        let stripped = Self::strip_prompt_template_markers(text);
        let mut lines = Vec::new();
        for line in stripped.lines() {
            let trimmed = Self::strip_prompt_role_prefixes(line.trim());
            if trimmed.is_empty() {
                continue;
            }
            lines.push(trimmed.to_string());
        }

        let joined = if lines.is_empty() {
            Self::strip_prompt_role_prefixes(stripped.trim()).to_string()
        } else {
            lines.join("\n")
        };
        joined.trim().to_string()
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

    fn strip_prompt_role_prefixes(text: &str) -> &str {
        let mut current = text.trim();

        loop {
            let lower = current.to_ascii_lowercase();
            let mut advanced = false;

            for prefix in [
                "assistant:",
                "user:",
                "system:",
                "analysis:",
                "final:",
                "answer:",
            ] {
                if lower.starts_with(prefix) {
                    current = current[prefix.len()..].trim_start();
                    advanced = true;
                    break;
                }
            }

            if advanced {
                continue;
            }

            for prefix in ["assistant", "user", "system", "analysis", "final", "answer"] {
                if lower == prefix {
                    current = "";
                    advanced = true;
                    break;
                }
                let with_space = format!("{prefix} ");
                if lower.starts_with(&with_space) {
                    current = current[prefix.len()..].trim_start();
                    advanced = true;
                    break;
                }
            }

            if !advanced {
                break;
            }
        }

        current.trim()
    }

    fn prepare_memory_text(text: &str, clip_chars: Option<usize>) -> String {
        let normalized = Self::normalize_model_message(text);
        let collapsed = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
        let safe = Self::display_safe_text(&collapsed);
        if let Some(max_chars) = clip_chars {
            Self::clip_chars(&safe, max_chars)
        } else {
            safe
        }
    }

    fn clip_chars(text: &str, max_chars: usize) -> String {
        let mut out = String::new();
        for ch in text.chars().take(max_chars) {
            out.push(ch);
        }
        if text.chars().count() > max_chars {
            out.push_str("...");
        }
        out
    }

    fn display_safe_text(text: &str) -> String {
        let normalized = Self::normalize_common_mojibake(text);
        let mut out = String::new();
        for ch in normalized.chars() {
            match ch {
                '\r' => {}
                '\n' => out.push('\n'),
                '\t' | '\u{00A0}' => out.push(' '),
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
                '│' | '┃' | '┆' | '┇' | '┊' | '┋' | '¦' | '｜' => out.push('|'),
                '─' | '━' | '═' | '﹘' | '﹣' | '－' => out.push('-'),
                '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' | '╭' | '╮' | '╰' | '╯'
                | '╳' | '╋' | '╂' | '╬' | '╠' | '╣' | '╦' | '╩' => out.push('+'),
                c if c.is_control() => {}
                c if c.is_ascii() => out.push(c),
                other => {
                    let transliterated = deunicode(&other.to_string());
                    if transliterated.trim().is_empty() {
                        out.push('?');
                    } else {
                        for fallback in transliterated.chars() {
                            match fallback {
                                '\r' => {}
                                '\n' => out.push('\n'),
                                '\t' => out.push(' '),
                                c if c.is_ascii_graphic() || c == ' ' => out.push(c),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        out.trim().to_string()
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

    fn warning_color(&self) -> egui::Color32 {
        if self.theme.name.eq_ignore_ascii_case("classic_light") {
            color_from_hex(&self.theme.accent)
        } else {
            egui::Color32::YELLOW
        }
    }

    fn render_teacher_homework_tools(&mut self, ui: &mut egui::Ui) {
        ui.heading("Teacher tools: Markdown homework");
        ui.label(
            "Workflow: write packs in .md (outgoing) → transcribe to JSON packs for students → submissions export to .json → convert to .md for marking.",
        );
        ui.label("Sharing: homework packs are simple files. You can share a pack .md or .json with other teachers/schools (include any referenced attachments).");

        let outgoing_dir = homework_markdown::homework_outgoing_dir(&self.base_path);
        let marking_dir = homework_markdown::homework_marking_dir(&self.base_path);
        let printables_dir = homework_markdown::homework_printables_dir(&self.base_path);
        let rubrics_dir = homework_markdown::homework_rubrics_dir(&self.base_path);
        ui.label(format!("Outgoing packs folder: {}", outgoing_dir.display()));
        ui.label(format!("Marking exports folder: {}", marking_dir.display()));
        ui.label(format!(
            "Student printables folder: {}",
            printables_dir.display()
        ));
        ui.label(format!("Teacher rubrics folder: {}", rubrics_dir.display()));

        ui.add(
            egui::TextEdit::multiline(&mut self.teacher_pack_request)
                .hint_text("Optional: Ask the local model to draft a homework pack (e.g., \"Year 7 Math fractions, 10 questions, include a word problem\").")
                .desired_width(f32::INFINITY)
                .desired_rows(3),
        );

        ui.horizontal(|ui| {
            if ui.button("Generate pack (.md) with AI").clicked() {
                let req = self.teacher_pack_request.trim().to_string();
                if req.is_empty() {
                    self.teacher_pack_status = Some("Type what you want the pack to include.".to_string());
                } else {
                    self.pulse_ecg(84.0, "Drafting a homework pack with the local model.");
                    let school_id = self
                        .current_pack
                        .as_ref()
                        .map(|p| p.school_id.clone())
                        .unwrap_or_else(|| "school".to_string());
                    let class_id = if self.settings.student.class_id.trim().is_empty() {
                        self.current_pack
                            .as_ref()
                            .map(|p| p.class_id.clone())
                            .unwrap_or_else(|| "class".to_string())
                    } else {
                        self.settings.student.class_id.trim().to_string()
                    };

                    let prompt = format!(
                        "{capsule}\nDefaults:\nversion: 1.0\nschool_id: {school}\nclass_id: {class}\ncreated_at: {now}\n\nTeacher request: {req}",
                        capsule = TEACHER_PACK_CAPSULE,
                        school = school_id,
                        class = class_id,
                        now = Utc::now().to_rfc3339(),
                        req = req
                    );

                    let result = panic::catch_unwind({
                        let mut model = self.settings.model.clone();
                        model.max_tokens = model.max_tokens.max(1024);
                        move || local_model::chat_completion(&model, &prompt)
                    });

                    match result {
                        Ok(Ok(text)) => {
                            self.teacher_pack_markdown = text.trim().to_string();
                            self.teacher_pack_status = if self.teacher_pack_markdown.is_empty() {
                                Some("The model returned an empty draft.".to_string())
                            } else {
                                Some(
                                    "Draft ready. Review/edit below, then save to outgoing."
                                        .to_string(),
                                )
                            };
                        }
                        Ok(Err(err)) => {
                            self.teacher_pack_status = Some(format!("AI generation failed: {err}"));
                        }
                        Err(_) => {
                            self.teacher_pack_status =
                                Some("Sorry, something went wrong while generating the pack.".to_string());
                        }
                    }
                }
            }

            let can_save = !self.teacher_pack_markdown.trim().is_empty();
            if ui
                .add_enabled(can_save, egui::Button::new("Save draft to outgoing"))
                .clicked()
            {
                self.pulse_ecg(42.0, "Saving a homework pack draft to outgoing.");
                let class_id = sanitize_filename_component(&self.settings.student.class_id);
                let ts = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
                let base_name = if class_id.trim().is_empty() {
                    format!("homework_pack_{ts}")
                } else {
                    format!("homework_pack_{class_id}_{ts}")
                };
                let _ = fs::create_dir_all(&outgoing_dir);
                let mut out_path = outgoing_dir.join(format!("{base_name}.md"));
                let mut n = 1usize;
                while out_path.exists() {
                    out_path = outgoing_dir.join(format!("{base_name}_{n}.md"));
                    n += 1;
                }
                match fs::write(&out_path, format!("{}\n", self.teacher_pack_markdown.trim_end()))
                {
                    Ok(_) => {
                        self.teacher_tools_status =
                            Some(format!("Saved outgoing pack: {}", out_path.display()));
                    }
                    Err(err) => {
                        self.teacher_tools_status =
                            Some(format!("Failed to save outgoing pack: {err}"));
                    }
                }
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Transcribe outgoing (.md → .json)").clicked() {
                self.pulse_ecg(66.0, "Transcribing outgoing homework markdown into JSON.");
                let defaults = PackMdDefaults {
                    version: "1.0".to_string(),
                    school_id: self
                        .current_pack
                        .as_ref()
                        .map(|p| p.school_id.clone())
                        .unwrap_or_else(|| "school".to_string()),
                    class_id: if self.settings.student.class_id.trim().is_empty() {
                        self.current_pack
                            .as_ref()
                            .map(|p| p.class_id.clone())
                            .unwrap_or_else(|| "class".to_string())
                    } else {
                        self.settings.student.class_id.trim().to_string()
                    },
                };

                match homework_markdown::transcribe_outgoing_packs(&self.base_path, &defaults) {
                    Ok(report) => {
                        self.teacher_tools_status = Some(format!(
                            "Outgoing → JSON: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        ));
                        if let Some(first) = report.errors.first() {
                            self.teacher_tools_status = Some(format!(
                                "{}. First error: {}",
                                self.teacher_tools_status.clone().unwrap_or_default(),
                                first
                            ));
                        }
                        self.resync_homework();
                        if let Some(pack) = self.current_pack.clone() {
                            apply_pack_policy(&mut self.settings, &pack);
                            let _ = save_settings(&self.settings, &self.base_path);
                        }
                    }
                    Err(err) => {
                        self.teacher_tools_status =
                            Some(format!("Outgoing → JSON failed: {err}"));
                    }
                }
            }

            if ui.button("Convert submissions (.json → .md)").clicked() {
                self.pulse_ecg(58.0, "Converting student submissions into marking markdown.");
                match homework_markdown::transcribe_completed_submissions_to_marking_md(
                    &self.base_path,
                    self.current_pack.as_ref(),
                ) {
                    Ok(report) => {
                        self.teacher_tools_status = Some(format!(
                            "Submissions → marking .md: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        ));
                        if let Some(first) = report.errors.first() {
                            self.teacher_tools_status = Some(format!(
                                "{}. First error: {}",
                                self.teacher_tools_status.clone().unwrap_or_default(),
                                first
                            ));
                        }
                    }
                    Err(err) => {
                        self.teacher_tools_status =
                            Some(format!("Submissions → marking .md failed: {err}"));
                    }
                }
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Export student printables (.md)").clicked() {
                let Some(pack) = self.current_pack.clone() else {
                    self.teacher_tools_status = Some(
                        "No pack loaded. Import/transcribe a pack first, then export.".to_string(),
                    );
                    return;
                };

                self.pulse_ecg(46.0, "Exporting student printables.");
                match homework_markdown::export_student_printables(&self.base_path, &pack) {
                    Ok(report) => {
                        self.teacher_tools_status = Some(format!(
                            "Student printables: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        ));
                        if let Some(first) = report.errors.first() {
                            self.teacher_tools_status = Some(format!(
                                "{}. First error: {}",
                                self.teacher_tools_status.clone().unwrap_or_default(),
                                first
                            ));
                        }
                    }
                    Err(err) => {
                        self.teacher_tools_status =
                            Some(format!("Student printables export failed: {err}"));
                    }
                }
            }

            if ui.button("Export teacher rubrics (.md)").clicked() {
                let Some(pack) = self.current_pack.clone() else {
                    self.teacher_tools_status = Some(
                        "No pack loaded. Import/transcribe a pack first, then export.".to_string(),
                    );
                    return;
                };

                self.pulse_ecg(46.0, "Exporting teacher rubrics.");
                match homework_markdown::export_teacher_rubrics(&self.base_path, &pack) {
                    Ok(report) => {
                        self.teacher_tools_status = Some(format!(
                            "Teacher rubrics: processed {}, wrote {}, skipped {}, failed {}",
                            report.processed, report.written, report.skipped, report.failed
                        ));
                        if let Some(first) = report.errors.first() {
                            self.teacher_tools_status = Some(format!(
                                "{}. First error: {}",
                                self.teacher_tools_status.clone().unwrap_or_default(),
                                first
                            ));
                        }
                    }
                    Err(err) => {
                        self.teacher_tools_status =
                            Some(format!("Teacher rubrics export failed: {err}"));
                    }
                }
            }
        });

        if let Some(status) = &self.teacher_pack_status {
            ui.colored_label(self.warning_color(), status);
        }
        if let Some(status) = &self.teacher_tools_status {
            ui.colored_label(self.warning_color(), status);
        }

        if !self.teacher_pack_markdown.trim().is_empty() {
            ui.separator();
            ui.label(RichText::new("Pack draft (.md)").strong());
            ui.add(
                egui::TextEdit::multiline(&mut self.teacher_pack_markdown)
                    .desired_width(f32::INFINITY)
                    .desired_rows(14),
            );
        }
    }

    fn render_teacher_revision_tools(&mut self, ui: &mut egui::Ui) {
        ui.heading("Teacher tools: Revision");
        ui.label(
            "Revision is separate from live homework. It reads from completed homework submissions and stores teacher revision materials under revision/.",
        );
        let revision_root = revision_dir(&self.base_path);
        let past_papers_root = revision_past_papers_dir(&self.base_path);
        ui.label(format!("Revision workspace: {}", revision_root.display()));
        ui.label(format!(
            "Past papers folder: {}",
            past_papers_root.display()
        ));
        ui.label(format!(
            "Completed submissions available to revision: {}",
            self.revision_sources.len()
        ));
        ui.label(format!("Imported past papers: {}", self.past_papers.len()));

        ui.horizontal(|ui| {
            if ui.button("Open Revision").clicked() {
                self.open_revision_workspace();
            }
            if ui
                .add_enabled(
                    !self.revision_sources.is_empty(),
                    egui::Button::new("Create revision pack (.md)"),
                )
                .clicked()
            {
                self.pulse_ecg(44.0, "Building a revision pack from completed homework.");
                match build_revision_pack_markdown(&self.base_path, &self.revision_sources) {
                    Ok(path) => {
                        self.teacher_revision_status =
                            Some(format!("Revision pack written to {}", path.display()));
                    }
                    Err(err) => {
                        self.teacher_revision_status =
                            Some(format!("Revision pack export failed: {err}"));
                    }
                }
            }
            if ui.button("Import past paper...").clicked() {
                if let Some(file) = FileDialog::new().pick_file() {
                    match import_past_paper(&self.base_path, &file) {
                        Ok(path) => {
                            self.resync_revision();
                            self.teacher_revision_status =
                                Some(format!("Past paper copied to {}", path.display()));
                        }
                        Err(err) => {
                            self.teacher_revision_status =
                                Some(format!("Past paper import failed: {err}"));
                        }
                    }
                }
            }
        });

        if !self.past_papers.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Past papers found").strong());
            for path in self.past_papers.iter().take(8) {
                ui.label(path.display().to_string());
            }
        }

        if let Some(status) = &self.teacher_revision_status {
            ui.colored_label(self.warning_color(), status);
        }
    }

    fn render_homework_dashboard(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical()
            .id_source("homework_dashboard_scroll")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.heading("Homework dashboard");
                ui.horizontal(|ui| {
                    if ui.button("Diagnostic / Health Check").clicked() {
                        self.open_diagnostics_tab();
                    }
                });
                if let Some(pack) = self.current_pack.clone() {
                    let visible_assignments = Self::unique_assignments_by_id(&pack);
                    ui.label(format!(
                        "Class: {} | Assignments: {}",
                        pack.class_id,
                        visible_assignments.len()
                    ));
                } else {
                    ui.label("No pack loaded yet. Import a pack to see class metrics.");
                }

                ui.separator();
                if !self.received_homework_inbox.is_empty() {
                    self.render_received_homework_pack_inbox(ui, "Homework pack inbox");
                    ui.separator();
                }
                self.render_teacher_homework_tools(ui);
                ui.separator();
                self.render_teacher_revision_tools(ui);
                if !self.received_revision_inbox.is_empty() {
                    ui.separator();
                    self.render_received_revision_pack_inbox(ui, "Revision pack inbox");
                }
                ui.separator();

                let all_entries = self.score_entries();
                let focused_entries: Vec<StudentScore> = if self.selected_students.is_empty() {
                    all_entries.clone()
                } else {
                    all_entries
                        .into_iter()
                        .filter(|s| self.selected_students.contains(&s.student_name))
                        .collect()
                };

                if focused_entries.is_empty() {
                    ui.label("No submissions found yet.");
                } else {
                    let (class_avg, per_student_avg, per_subject_avg) =
                        aggregate_scores(&focused_entries);
                    ui.separator();
                    ui.label("Class / selection average");
                    ui.add(
                        ProgressBar::new(class_avg / 100.0)
                            .fill(score_color(class_avg))
                            .text(format!("{:.1} / 100", class_avg)),
                    );

                    ui.horizontal(|ui| {
                        ui.label("Students:");
                        if ui.button("Clear selection").clicked() {
                            self.selected_students.clear();
                        }
                    });
                    ScrollArea::vertical()
                        .id_source("homework_dashboard_students_scroll")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for (name, avg) in &per_student_avg {
                                ui.push_id(("homework_dashboard_student", name), |ui| {
                                    let selected = self.selected_students.contains(name);
                                    let label = format!("{name} ({avg:.1})");
                                    if ui.selectable_label(selected, label).clicked() {
                                        if selected {
                                            self.selected_students.remove(name);
                                        } else {
                                            self.selected_students.insert(name.clone());
                                        }
                                    }
                                });
                            }
                        });

                    ui.separator();
                    ui.label("Subject metrics");
                    for (subj, score) in &per_subject_avg {
                        ui.horizontal(|ui| {
                            ui.label(subj);
                            ui.add(
                                ProgressBar::new(*score / 100.0)
                                    .fill(score_color(*score))
                                    .text(format!("{score:.1}")),
                            );
                        });
                    }
                }

                if !self.submissions.is_empty() {
                    ui.separator();
                    ui.heading("Submissions found locally");
                    ScrollArea::vertical()
                        .id_source("homework_dashboard_submissions_scroll")
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for row in self.submission_rows() {
                                ui.push_id(
                                    (
                                        "homework_dashboard_submission_row",
                                        &row.assignment_id,
                                        &row.student_id,
                                        &row.submitted_at,
                                    ),
                                    |ui| {
                                        let label = format!(
                                            "{} ({}) - {} ({}) | subj: {} | score: {} | {}",
                                            row.assignment_title,
                                            row.assignment_id,
                                            row.student_name,
                                            row.student_id,
                                            row.subject,
                                            row.score,
                                            row.feedback
                                        );
                                        ui.label(label).on_hover_text(format!(
                                            "Assignment ID: {} | Student ID: {} | Submitted: {}",
                                            row.assignment_id, row.student_id, row.submitted_at
                                        ));
                                    },
                                );
                            }
                        });
                }
            });
    }

    fn build_revision_prompt(&self, source: &RevisionSource, user_input: &str) -> String {
        let notes = if self.revision_notes.trim().is_empty() {
            "No saved revision notes yet.".to_string()
        } else {
            self.revision_notes.trim().to_string()
        };
        let instructions = source
            .instructions_md
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or("No assignment snapshot stored in this submission.");
        let feedback = source
            .ai_feedback
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or("No revision focus note recorded.");

        format!(
            "{capsule}\nRevision source:\n- Assignment: {title} ({id})\n- Subject: {subject}\n- Year level: {year}\n- Submitted: {submitted}\n\nInternal guidance (do not quote these notes back to the student as diagnostics):\n- Revision focus note: {feedback}\n\nOriginal assignment snapshot:\n{instructions}\n\nStudent's previous submission:\n{submission}\n\nSaved revision notes:\n{notes}\n\nStudent request: {request}\nRespond with one short revision-focused reply.",
            capsule = REVISION_CHAT_CAPSULE,
            title = source.assignment_title.as_str(),
            id = source.assignment_id.as_str(),
            subject = source.subject.as_str(),
            year = source.year_level.as_str(),
            submitted = source.submitted_at.as_str(),
            feedback = feedback,
            instructions = instructions,
            submission = if source.answers_text.trim().is_empty() {
                "No submission text stored."
            } else {
                source.answers_text.trim()
            },
            notes = notes,
            request = user_input
        )
    }

    fn revision_confidence_label(confidence: i32) -> &'static str {
        match confidence.clamp(0, 100) {
            0..=30 => "Need more practice",
            31..=55 => "Getting there",
            56..=80 => "Mostly comfortable",
            _ => "Ready to move on",
        }
    }

    fn revision_confidence_bucket(confidence: i32) -> i32 {
        match confidence.clamp(0, 100) {
            0..=30 => 0,
            31..=55 => 1,
            56..=80 => 2,
            _ => 3,
        }
    }

    fn revision_source_label(source: &RevisionSource, teacher_unlocked: bool) -> String {
        if teacher_unlocked {
            format!(
                "{} ({}) | score {}",
                source.assignment_title,
                source.assignment_id,
                source
                    .ai_score
                    .map(|score| score.to_string())
                    .unwrap_or_else(|| "-".to_string())
            )
        } else {
            format!("{} ({})", source.assignment_title, source.assignment_id)
        }
    }

    fn revision_social_cue_reply(text: &str) -> Option<&'static str> {
        let normalized: String = text
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        match normalized.as_str() {
            "thanks" | "thank you" | "thankyou" | "thanks chatty" | "thank you chatty"
            | "got it" | "ok thanks" | "okay thanks" | "cheers" | "bye" | "goodbye" => {
                Some("You're welcome. Ask if you want to revise one more part.")
            }
            _ => None,
        }
    }

    fn handle_revision_send(&mut self) {
        if self.revision_chat_input.trim().is_empty() {
            return;
        }
        let Some(source) = self.selected_revision_source().cloned() else {
            return;
        };

        let user_msg = self.revision_chat_input.trim().to_string();
        if let Some(reply) = Self::revision_social_cue_reply(&user_msg) {
            self.revision_chat_log
                .push(("You".to_string(), user_msg.clone()));
            self.revision_chat_log
                .push(("Chatty".to_string(), reply.to_string()));
            self.revision_chat_input.clear();
            self.pulse_ecg(8.0, "Handled a short revision social cue locally.");
            return;
        }

        self.pulse_ecg(68.0, "Generating revision help with the local model.");
        self.revision_chat_log
            .push(("You".to_string(), user_msg.clone()));
        self.revision_chat_log
            .push(("Chatty".to_string(), "...".to_string()));

        let result = panic::catch_unwind({
            let settings = self.settings.clone();
            let prompt = self.build_revision_prompt(&source, &user_msg);
            move || generate_answer(&settings, &prompt)
        });

        let reply = match result {
            Ok(text) => Self::normalize_model_message(&text),
            Err(_) => "Sorry, I ran into an error while answering.".to_string(),
        };

        if let Some(last) = self.revision_chat_log.last_mut() {
            last.1 = reply;
        }
        self.revision_chat_input.clear();
    }

    fn render_revision_workspace(&mut self, ui: &mut egui::Ui) {
        ui.heading("Revision");
        ui.label(
            "Revision is separate from live homework. It uses completed homework as source material and stores notes or progress under revision/.",
        );
        if ui.button("Diagnostic / Health Check").clicked() {
            self.open_diagnostics_tab();
        }

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                ui.separator();
                ui.label(format!(
                    "Completed homework sources: {} | Past papers: {}",
                    self.revision_sources.len(),
                    self.past_papers.len()
                ));
                if self.teacher_unlocked {
                    ui.label(
                        "Spaced repetition view: lower scores, saved low confidence, and feedback-heavy items naturally rise to the top.",
                    );
                } else {
                    ui.label(
                        "Revision gently lifts the topics you seem to need most, so the right items stay near the top.",
                    );
                }

                if !self.received_revision_inbox.is_empty() {
                    self.render_received_revision_pack_inbox(ui, "Received revision packs");
                    ui.separator();
                }

                if self.revision_sources.is_empty() {
                    ui.colored_label(
                        self.warning_color(),
                        "No completed homework was found for revision yet. Submit homework from Home first, or add past papers below.",
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Revision source:");
                        let revision_choices = self.revision_sources.clone();
                        let current = self
                            .selected_revision_source()
                            .map(|source| Self::revision_source_label(source, self.teacher_unlocked))
                            .unwrap_or_else(|| "Select revision source".to_string());
                        egui::ComboBox::from_id_source("revision_source_select")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for source in &revision_choices {
                                    let label =
                                        Self::revision_source_label(source, self.teacher_unlocked);
                                    if ui
                                        .selectable_label(
                                            self.selected_revision.as_ref()
                                                == Some(&source.revision_key),
                                            label,
                                        )
                                        .clicked()
                                    {
                                        self.selected_revision =
                                            Some(source.revision_key.clone());
                                        self.revision_chat_log.clear();
                                        self.sync_revision_editor_from_selection();
                                    }
                                }
                            });
                    });

                    if let Some(source) = self.selected_revision_source().cloned() {
                        let saved_progress =
                            self.revision_progress.get(&source.revision_key).cloned();
                        ui.separator();
                        ui.label(
                            RichText::new(&source.assignment_title)
                                .heading()
                                .color(color_from_hex(&self.theme.accent)),
                        );
                        ui.label(format!(
                            "{} | Year {} | Submitted {}",
                            source.subject, source.year_level, source.submitted_at
                        ));
                        if self.teacher_unlocked {
                            ui.label(format!(
                                "Revision priority: {} | AI score: {}",
                                revision_priority(&source),
                                source
                                    .ai_score
                                    .map(|score| score.to_string())
                                    .unwrap_or_else(|| "-".to_string())
                            ));
                        }
                        if let Some(progress) = &saved_progress {
                            if self.teacher_unlocked {
                                ui.label(format!(
                                    "Saved confidence: {} / 100 | Reviews logged: {}",
                                    progress.confidence, progress.review_count
                                ));
                            } else {
                                ui.label(format!(
                                    "Confidence check-in: {}",
                                    Self::revision_confidence_label(progress.confidence)
                                ));
                            }
                            if !progress.last_reviewed_at.trim().is_empty() {
                                ui.label(format!(
                                    "Last reviewed: {}",
                                    progress.last_reviewed_at
                                ));
                            }
                        }
                        if self.teacher_unlocked {
                            if let Some(feedback) = source.ai_feedback.as_deref() {
                                ui.colored_label(
                                    self.warning_color(),
                                    format!("Focus from feedback: {}", feedback.trim()),
                                );
                            }
                        }
                        if let Some(instructions) = source.instructions_md.as_deref() {
                            self.render_markdown_card(
                                ui,
                                "Original assignment snapshot",
                                instructions,
                            );
                        } else if self.teacher_unlocked {
                            ui.colored_label(
                                self.warning_color(),
                                "This older submission does not include an embedded assignment snapshot, so revision is based on the submission and feedback only.",
                            );
                        }
                        else {
                            ui.colored_label(
                                self.warning_color(),
                                "This older submission does not include the original assignment snapshot, so revision is based on your submission only.",
                            );
                        }

                        if !source.answers_text.trim().is_empty() {
                            ui.add_space(6.0);
                            self.render_markdown_card(
                                ui,
                                "Your submitted work",
                                &source.answers_text,
                            );
                        }

                        ui.separator();
                        ui.heading("Revision notes");
                        if self.teacher_unlocked {
                            ui.add(
                                egui::Slider::new(&mut self.revision_confidence, 0..=100)
                                    .text("Confidence"),
                            );
                        } else {
                            ui.label("How ready do you feel about this topic?");
                            let selected_bucket =
                                Self::revision_confidence_bucket(self.revision_confidence);
                            ui.horizontal_wrapped(|ui| {
                                for (bucket, label, value) in [
                                    (0, "Need more practice", 20),
                                    (1, "Getting there", 45),
                                    (2, "Mostly comfortable", 70),
                                    (3, "Ready to move on", 90),
                                ] {
                                    let selected = selected_bucket == bucket;
                                    if ui.selectable_label(selected, label).clicked() {
                                        self.revision_confidence = value;
                                    }
                                }
                            });
                        }
                        ui.add(
                            egui::TextEdit::multiline(&mut self.revision_notes)
                                .desired_width(f32::INFINITY)
                                .desired_rows(6)
                                .hint_text("Write what you need to revisit, rules to remember, or your own summary."),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Save revision notes").clicked() {
                                let mut review_count = saved_progress
                                    .as_ref()
                                    .map(|progress| progress.review_count)
                                    .unwrap_or(0);
                                review_count += 1;
                                let progress = RevisionProgress {
                                    revision_key: source.revision_key.clone(),
                                    notes: self.revision_notes.trim().to_string(),
                                    confidence: self.revision_confidence.clamp(0, 100),
                                    review_count,
                                    last_reviewed_at: Utc::now().to_rfc3339(),
                                };
                                match save_revision_progress(&self.base_path, &progress) {
                                    Ok(path) => {
                                        self.revision_progress.insert(
                                            progress.revision_key.clone(),
                                            progress,
                                        );
                                        self.resync_revision();
                                        self.revision_status = Some(format!(
                                            "Saved revision notes to {}",
                                            path.display()
                                        ));
                                        self.pulse_ecg(22.0, "Saved revision notes.");
                                    }
                                    Err(err) => {
                                        self.revision_status = Some(format!(
                                            "Revision save failed: {err}"
                                        ));
                                    }
                                }
                            }
                            if ui.button("Reload saved notes").clicked() {
                                self.sync_revision_editor_from_selection();
                                self.revision_status =
                                    Some("Reloaded saved revision notes.".to_string());
                            }
                            if ui.button("Clear draft").clicked() {
                                self.revision_notes.clear();
                                self.revision_confidence = 50;
                            }
                        });
                        if let Some(status) = &self.revision_status {
                            ui.label(status);
                        }

                        ui.separator();
                        ui.heading("Revision helper");
                        ui.label(
                            "Open helper mode: this work is already submitted, so Chatty can explain more directly while staying focused on learning.",
                        );
                        egui::Frame::none()
                            .fill(color_from_hex(&self.theme.surface))
                            .stroke(egui::Stroke::new(
                                1.0,
                                color_from_hex(&self.theme.border),
                            ))
                            .rounding(egui::Rounding::same(6.0))
                            .inner_margin(egui::vec2(10.0, 8.0))
                            .show(ui, |ui| {
                                ui.set_min_height(160.0);
                                let max_width = ui.available_width() * 0.98;
                                ui.set_max_width(max_width);
                                self.render_chat_messages(
                                    ui,
                                    &self.revision_chat_log,
                                    max_width,
                                );
                            });
                        ui.horizontal(|ui| {
                            let input = ui.add(
                                egui::TextEdit::singleline(&mut self.revision_chat_input)
                                    .hint_text("Ask about this completed work, a mistake, or how to revise it..."),
                            );
                            if input.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                self.handle_revision_send();
                            }
                            if ui.button("Send").clicked() {
                                self.handle_revision_send();
                            }
                        });
                    }
                }

                if !self.past_papers.is_empty() {
                    ui.separator();
                    ui.heading("Past papers");
                    for path in &self.past_papers {
                        ui.label(path.display().to_string());
                    }
                }
            });
    }

    fn render_module_tab(&mut self, ui: &mut egui::Ui, tab_idx: usize) {
        let Some(module_preview) = self.tabs.get(tab_idx).and_then(|tab| match &tab.kind {
            TabKind::Module { module, .. } => Some(module.clone()),
            _ => None,
        }) else {
            return;
        };

        if module_preview.manifest.id == "homework_dashboard" && !self.teacher_unlocked {
            ui.colored_label(
                self.warning_color(),
                "Teacher view is locked. Unlock via the Teacher menu to open this dashboard.",
            );
            return;
        }

        if let Some(visual) = module_preview
            .manifest
            .visual_load
            .clone()
            .filter(|visual| visual.hosts_native_window())
        {
            self.render_hosted_module_tab(ui, &module_preview, &visual);
            return;
        }

        ui.heading(&module_preview.manifest.title);
        if let Some(desc) = &module_preview.manifest.description {
            ui.label(desc);
        }
        ui.separator();

        match module_preview.manifest.entry.clone() {
            Some(ModuleEntry::BuiltinPanel { target }) => match target.as_str() {
                "homework_dashboard" => self.render_homework_dashboard(ui),
                "homework_assignments" | "revision_workspace" => self.render_revision_workspace(ui),
                _ => {
                    ui.label(format!("Builtin panel stub: {}", target));
                }
            },
            Some(ModuleEntry::Markdown { path }) => {
                let cached = self.tabs.get(tab_idx).and_then(|tab| match &tab.kind {
                    TabKind::Module { cached_text, .. } => cached_text.clone(),
                    _ => None,
                });

                if cached.is_none() {
                    let full_path = module_preview.folder.join(&path);
                    let loaded = fs::read_to_string(&full_path).ok();
                    if let Some(tab) = self.tabs.get_mut(tab_idx) {
                        if let TabKind::Module { cached_text, .. } = &mut tab.kind {
                            *cached_text = loaded.clone();
                        }
                    }
                }

                let text = self.tabs.get(tab_idx).and_then(|tab| match &tab.kind {
                    TabKind::Module { cached_text, .. } => cached_text.clone(),
                    _ => None,
                });

                if let Some(text) = text {
                    render_markdown(ui, &text);
                } else {
                    ui.label("Could not load markdown file.");
                }
            }
            Some(ModuleEntry::StaticHtml { path }) => {
                ui.label(format!("Static HTML surface declared at: {}", path));
                ui.small(
                    "This module is not advertising a hosted visual loader yet. Add a `visual_load.json` file if you want Chatty-EDU to host the real browser UI in-tab.",
                );
            }
            Some(ModuleEntry::ExternalProcess { command, args }) => {
                if self.allow_external_process {
                    ui.label(format!(
                        "External process would run: {} {:?}",
                        command, args
                    ));
                    ui.label("Process launching is stubbed for safety.");
                } else {
                    ui.colored_label(
                        self.warning_color(),
                        "External processes are disabled in safe mode.",
                    );
                }
            }
            None => {
                ui.small(
                    "This module does not declare a fallback `entry`. If it has a real standalone UI, add `visual_load.json` so Chatty-EDU can host it directly in this tab.",
                );
                self.render_module_bridge_panel(ui, &module_preview);
            }
        }
    }

    fn render_hosted_module_tab(
        &mut self,
        ui: &mut egui::Ui,
        module: &LoadedModule,
        visual: &ModuleVisualLoad,
    ) {
        let module_id = module.manifest.id.clone();
        let (running, waiting, status) = {
            let host = self
                .module_hosts
                .entry(module_id.clone())
                .or_insert_with(ModuleHostState::default);
            (
                host.is_running(),
                host.is_waiting_for_window(),
                host.status.clone(),
            )
        };

        let mut build_clicked = false;
        let mut restart_clicked = false;
        let mut close_clicked = false;
        let mut launch_clicked = false;
        let mut open_folder_clicked = false;

        ui.horizontal_wrapped(|ui| {
            ui.heading(&module.manifest.title);
            ui.separator();
            ui.small(
                module
                    .manifest
                    .description
                    .as_deref()
                    .unwrap_or("Hosted module UI"),
            );

            if visual.build.is_some() {
                ui.separator();
                build_clicked = ui.button("Build UI").clicked();
            }

            ui.separator();
            if running {
                restart_clicked = ui.button("Restart UI").clicked();
                close_clicked = ui.button("Close module app").clicked();
            } else {
                launch_clicked = ui.button("Launch in tab").clicked();
            }
            open_folder_clicked = ui.button("Open module folder").clicked();

            ui.separator();
            ui.small(status);
        });

        if build_clicked {
            let host = self
                .module_hosts
                .entry(module_id.clone())
                .or_insert_with(ModuleHostState::default);
            if let Err(err) = host.start_build(&module.folder, visual) {
                host.status = err;
            }
        }

        if restart_clicked {
            let host = self
                .module_hosts
                .entry(module_id.clone())
                .or_insert_with(ModuleHostState::default);
            host.force_stop();
            if let Err(err) = host.launch(&module.folder, visual) {
                host.status = err;
            }
        }

        if close_clicked {
            let host = self
                .module_hosts
                .entry(module_id.clone())
                .or_insert_with(ModuleHostState::default);
            host.request_close(visual);
            self.close_pending_modules.insert(module_id.clone());
        }

        if launch_clicked {
            let host = self
                .module_hosts
                .entry(module_id.clone())
                .or_insert_with(ModuleHostState::default);
            if let Err(err) = host.launch(&module.folder, visual) {
                host.status = err;
            }
        }

        if open_folder_clicked {
            open_path_in_explorer(&module.folder);
        }

        if !visual.notes.trim().is_empty() {
            ui.small(visual.notes.trim());
        }

        ui.add_space(6.0);
        egui::CollapsingHeader::new("Chatty-EDU bridge")
            .default_open(false)
            .show(ui, |ui| {
                ui.small(
                    "This is the compatibility loop only. The module keeps owning its real UI and state; Chatty-EDU just hosts it in-tab and reads the optional bridge/status plug when the module reports one.",
                );
                self.render_module_bridge_panel(ui, module);
            });

        ui.add_space(8.0);
        let available = ui.available_size();
        let desired = egui::vec2(available.x.max(240.0), available.y.max(320.0));
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, egui::Color32::WHITE);
        ui.painter()
            .rect_stroke(rect, 0.0, egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY));

        self.set_module_host_target(&module_id, rect, ui.ctx().pixels_per_point());

        let message = if running {
            if waiting {
                if visual.is_webview() {
                    "Launching hosted webview..."
                } else {
                    "Launching native module window..."
                }
            } else if visual.is_webview() {
                "Hosted webview is live here."
            } else {
                "Hosted native UI is live here."
            }
        } else if visual.is_webview() {
            "Hosted webview is not running yet."
        } else {
            "Hosted native UI is not running yet."
        };

        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            message,
            egui::TextStyle::Body.resolve(ui.style()),
            egui::Color32::DARK_GRAY,
        );
    }

    fn render_module_bridge_panel(&mut self, ui: &mut egui::Ui, module: &LoadedModule) {
        let declared_network_caps = module.manifest.network_capabilities.as_ref();
        let status_path = bridge_status_path(&module.folder);
        let log_sources_path = bridge_log_sources_path(&module.folder);
        let shared_state_path = bridge_shared_state_path(&module.folder);
        let incoming_shared_state_path = bridge_incoming_shared_state_path(&module.folder);
        let incoming_assets_dir = bridge_incoming_assets_dir(&module.folder);
        let shared_room_state_path = bridge_shared_room_state_path(&module.folder);
        let shared_room_events_path = bridge_shared_room_events_path(&module.folder);
        let outgoing_room_events_path = bridge_outgoing_room_events_path(&module.folder);
        let room_capable = declared_network_caps
            .map(|caps| {
                caps.has(ModuleNetworkFeature::RoomAware)
                    || caps.has(ModuleNetworkFeature::Multiplayer)
            })
            .unwrap_or(false);

        egui::CollapsingHeader::new("Declared network capabilities")
            .default_open(false)
            .show(ui, |ui| {
                if let Some(caps) = declared_network_caps {
                    ui.small(
                        "Optional contract: this tells Chatty-EDU which network lanes the module intentionally supports, so classroom sharing stays predictable and portable.",
                    );
                    if !caps.features.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for feature in &caps.features {
                                ui.label(RichText::new(feature.label()).small().monospace());
                            }
                        });
                    }
                    if !caps.asset_lanes.is_empty() {
                        ui.add_space(6.0);
                        ui.label("Declared asset lanes");
                        for lane in &caps.asset_lanes {
                            ui.group(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.strong(lane.label.trim());
                                    ui.small(format!(
                                        "[{} | {} | {}]",
                                        lane.lane_id,
                                        lane.direction.label(),
                                        lane.delivery_mode.label()
                                    ));
                                });
                                let mut summary_bits = Vec::new();
                                if !lane.artifact_kinds.is_empty() {
                                    summary_bits
                                        .push(format!("Kinds: {}", lane.artifact_kinds.join(", ")));
                                }
                                if !lane.accepted_content_types.is_empty() {
                                    summary_bits.push(format!(
                                        "Content: {}",
                                        lane.accepted_content_types.join(", ")
                                    ));
                                }
                                if let Some(max_bytes) = lane.max_bytes {
                                    summary_bits.push(format!(
                                        "Max: {}",
                                        format_network_transfer_size(max_bytes)
                                    ));
                                }
                                summary_bits.push(if lane.replayable {
                                    "Replayable".to_string()
                                } else {
                                    "Not replayable".to_string()
                                });
                                ui.small(summary_bits.join(" | "));
                                for note in &lane.notes {
                                    ui.small(format!("Note: {}", note));
                                }
                            });
                        }
                    }
                    for note in &caps.notes {
                        ui.small(format!("Note: {}", note));
                    }
                    if caps.features.is_empty() && caps.asset_lanes.is_empty() && caps.notes.is_empty() {
                        ui.small("This module's capability block is present but currently empty.");
                    }
                } else {
                    ui.small(
                        "No `network_capabilities.json` declared yet. Chatty-EDU will keep falling back to bridge-file presence and safe manual controls.",
                    );
                }
            });

        if room_capable {
            egui::CollapsingHeader::new("Shared room lane")
                .default_open(false)
                .show(ui, |ui| {
                    let multiplayer = declared_network_caps
                        .map(|caps| caps.has(ModuleNetworkFeature::Multiplayer))
                        .unwrap_or(false);
                    if self.shared_chat_scope_matches_module(&module.manifest.id) {
                        ui.small(format!(
                            "The shared room is currently focused on this module: {}.",
                            self.shared_chat_scope_label()
                        ));
                    } else {
                        ui.small(
                            "This module can opt into the shared-room lane. Use the buttons below when you want the room policy to follow this module cleanly.",
                        );
                    }
                    ui.horizontal_wrapped(|ui| {
                        let button_label = if multiplayer {
                            "Use this module as multiplayer room"
                        } else {
                            "Use this module in shared room"
                        };
                        if ui.button(button_label).clicked() {
                            self.set_shared_chat_scope_module(
                                module.manifest.id.clone(),
                                module.manifest.title.clone(),
                                multiplayer,
                            );
                            self.broadcast_shared_chat_policy(
                                "Room scope moved to this module.",
                            );
                        }
                        if ui.button("Return room to general").clicked() {
                            self.set_shared_chat_scope_general();
                            self.broadcast_shared_chat_policy(
                                "Room scope returned to the general lane.",
                            );
                        }
                        if ui.button("Open shared room controls").clicked() {
                            self.focus_networking_section(NetworkingFocusSection::SharedRoom);
                            if let Some(index) = self
                                .tabs
                                .iter()
                                .position(|tab| matches!(tab.kind, TabKind::Networking))
                            {
                                self.active_tab = index;
                            }
                        }
                        if self.shared_chat_scope_matches_module(&module.manifest.id)
                            && !self.networking_shared_chat_policy.session_active
                            && ui.button("Start room session now").clicked()
                        {
                            if let Some(module_name) = self.begin_shared_chat_module_session() {
                                self.broadcast_shared_chat_policy(&format!(
                                    "Started classroom module session for {module_name}."
                                ));
                            }
                        } else if self.shared_chat_scope_matches_module(&module.manifest.id)
                            && self.networking_shared_chat_policy.session_active
                            && ui.button("End room session now").clicked()
                        {
                            let label = self
                                .networking_shared_chat_policy
                                .session_label
                                .trim()
                                .to_string();
                            self.end_shared_chat_module_session();
                            self.broadcast_shared_chat_policy(&format!(
                                "Ended {}.",
                                if label.is_empty() {
                                    "the module session".to_string()
                                } else {
                                    label
                                }
                            ));
                        }
                    });
                });
        }

        ui.horizontal(|ui| {
            if ui.button("Open bridge folder").clicked() {
                open_path_in_explorer(status_path.parent().unwrap_or(&module.folder));
            }
            if status_path.is_file() && ui.button("Open status.json").clicked() {
                open_path_in_explorer(&status_path);
            }
            if log_sources_path.is_file() && ui.button("Open log_sources.json").clicked() {
                open_path_in_explorer(&log_sources_path);
            }
            if shared_state_path.is_file() && ui.button("Open shared_state.json").clicked() {
                open_path_in_explorer(&shared_state_path);
            }
            if incoming_shared_state_path.is_file()
                && ui.button("Open incoming_shared_state.json").clicked()
            {
                open_path_in_explorer(&incoming_shared_state_path);
            }
            if shared_room_state_path.is_file()
                && ui.button("Open shared_room_state.json").clicked()
            {
                open_path_in_explorer(&shared_room_state_path);
            }
            if shared_room_events_path.is_file()
                && ui.button("Open shared_room_events.json").clicked()
            {
                open_path_in_explorer(&shared_room_events_path);
            }
            if outgoing_room_events_path.is_file()
                && ui.button("Open outgoing_room_events.json").clicked()
            {
                open_path_in_explorer(&outgoing_room_events_path);
            }
            if incoming_assets_dir.is_dir() && ui.button("Open incoming assets").clicked() {
                open_path_in_explorer(&incoming_assets_dir);
            }
        });

        if let Some(status) = self.read_module_bridge_status(&module.manifest.id, &module.folder) {
            if !status.summary.trim().is_empty() {
                ui.label(RichText::new("Module summary").strong());
                ui.group(|ui| {
                    ui.label(status.summary.trim());
                });
            }

            if !status.snapshot.trim().is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Snapshot").strong());
                let mut snapshot = status.snapshot.clone();
                ScrollArea::vertical()
                    .id_source(format!("module_bridge_snapshot_{}", module.manifest.id))
                    .max_height(180.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut snapshot)
                                .desired_rows(8)
                                .interactive(false),
                        );
                    });
            }
        } else {
            ui.small(
                "No bridge status yet. That is okay: the module stays standalone either way. Add the optional bridge plug if you want Chatty-EDU to pick up recent module context automatically.",
            );
        }

        if room_capable {
            ui.add_space(6.0);
            ui.label(RichText::new("Shared room state").strong());
            match read_bridge_shared_room_state(&module.folder) {
                Ok(Some(room_state)) => {
                    ui.small(format!(
                        "Last room-state update: {}",
                        room_state.updated_at_unix_ms
                    ));
                    ui.small(format!(
                        "Scope: {}",
                        if room_state.scope_kind.trim() == "module"
                            && !room_state.scope_module_name.trim().is_empty()
                        {
                            if room_state.scope_multiplayer {
                                format!("{} (multiplayer)", room_state.scope_module_name.trim())
                            } else {
                                format!("{} (module)", room_state.scope_module_name.trim())
                            }
                        } else {
                            "General room".to_string()
                        }
                    ));
                    ui.small(format!(
                        "Active for this module: {}",
                        if room_state.active_for_module {
                            "yes"
                        } else {
                            "no"
                        }
                    ));
                    ui.small(format!(
                        "Turn mode: {} | AI mode: {}",
                        if room_state.turn_mode.trim().is_empty() {
                            "(unset)"
                        } else {
                            room_state.turn_mode.trim()
                        },
                        if room_state.ai_mode.trim().is_empty() {
                            "(unset)"
                        } else {
                            room_state.ai_mode.trim()
                        }
                    ));
                    if room_state.session_active {
                        ui.small(format!(
                            "Session: {} | revision {}{}",
                            if room_state.session_label.trim().is_empty() {
                                if room_state.session_id.trim().is_empty() {
                                    "(unnamed session)"
                                } else {
                                    room_state.session_id.trim()
                                }
                            } else {
                                room_state.session_label.trim()
                            },
                            room_state.session_revision.max(1),
                            if room_state.host_authoritative {
                                " | host-authoritative"
                            } else {
                                ""
                            }
                        ));
                    } else {
                        ui.small("Session: inactive");
                    }
                    if room_state.teacher_override {
                        ui.small("Teacher override is active for this room.");
                    }
                    if !room_state.host_device_name.trim().is_empty() {
                        ui.small(format!("Host: {}", room_state.host_device_name.trim()));
                    }
                    if !room_state.turn_holder_device_name.trim().is_empty() {
                        ui.small(format!(
                            "Turn holder: {}",
                            room_state.turn_holder_device_name.trim()
                        ));
                    }
                    ui.small(format!(
                        "Connected peers in room: {}",
                        room_state.connected_peer_count
                    ));
                    ui.small(format!(
                        "Participants visible to module: {}",
                        room_state.participant_count
                    ));
                    if !room_state.participants.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for participant in room_state.participants.iter().take(8) {
                                let label = if participant.device_name.trim().is_empty() {
                                    participant.device_id.trim()
                                } else {
                                    participant.device_name.trim()
                                };
                                ui.small(if participant.is_local {
                                    format!("(local) {label}")
                                } else {
                                    label.to_string()
                                });
                            }
                        });
                    }
                    if !room_state.summary.trim().is_empty() {
                        ui.group(|ui| {
                            ui.label(room_state.summary.trim());
                        });
                    }
                }
                Ok(None) => {
                    ui.small(
                        "No shared_room_state.json yet. Once the classroom room is active, Chatty-EDU will mirror that room policy here for room-aware or multiplayer modules.",
                    );
                }
                Err(err) => {
                    ui.small(format!("Could not read shared_room_state.json: {err}"));
                }
            }

            ui.add_space(6.0);
            ui.label(RichText::new("Recent shared room events").strong());
            match read_bridge_shared_room_events(&module.folder) {
                Ok(Some(events)) => {
                    ui.small(format!(
                        "Last event sync: {} | {} event(s)",
                        events.updated_at_unix_ms,
                        events.events.len()
                    ));
                    for event in events.events.iter().rev().take(8) {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(if event.label.trim().is_empty() {
                                    event.event_type.trim()
                                } else {
                                    event.label.trim()
                                });
                                ui.small(format!(
                                    "{} | {}",
                                    if event.from_device_name.trim().is_empty() {
                                        "(unknown sender)"
                                    } else {
                                        event.from_device_name.trim()
                                    },
                                    event.received_at_unix_ms
                                ));
                            });
                            if !event.payload_text.trim().is_empty() {
                                ui.label(event.payload_text.trim());
                            } else {
                                ui.small("(no text payload)");
                            }
                        });
                    }
                }
                Ok(None) => {
                    ui.small(
                        "No shared_room_events.json yet. Room-aware modules can read a recent event feed here once peers start emitting lightweight room events.",
                    );
                }
                Err(err) => {
                    ui.small(format!("Could not read shared_room_events.json: {err}"));
                }
            }
            match read_bridge_outgoing_room_events(&module.folder) {
                Ok(events) if !events.is_empty() => {
                    ui.small(format!("Queued outgoing room events: {}", events.len()));
                }
                Ok(_) => {}
                Err(err) => {
                    ui.small(format!("Could not read outgoing_room_events.json: {err}"));
                }
            }
        }

        ui.add_space(6.0);
        ui.label(RichText::new("Shared session state").strong());
        if let Some(shared_state) =
            self.read_module_bridge_shared_state(&module.manifest.id, &module.folder)
        {
            let can_publish_shared_state = module
                .manifest
                .network_capabilities
                .as_ref()
                .map(|caps| caps.has(ModuleNetworkFeature::SharedStatePublish))
                .unwrap_or(true);
            let can_receive_shared_state = module
                .manifest
                .network_capabilities
                .as_ref()
                .map(|caps| caps.has(ModuleNetworkFeature::SharedStateReceive))
                .unwrap_or(true);
            let tracker = self
                .module_session_trackers
                .get(&module.manifest.id)
                .cloned();
            if shared_state.updated_at_unix_ms > 0 {
                ui.small(format!(
                    "Last shared-state update: {}",
                    shared_state.updated_at_unix_ms
                ));
            }
            if let Some(tracker) = &tracker {
                ui.small(format!(
                    "Current shared session: {} | revision {}",
                    tracker.session_id, tracker.last_revision
                ));
            } else if !shared_state.session_id.trim().is_empty() {
                ui.small(format!(
                    "Current shared session: {} | revision {}",
                    shared_state.session_id, shared_state.session_revision
                ));
            }
            if !shared_state.summary.trim().is_empty() {
                ui.group(|ui| {
                    ui.label(shared_state.summary.trim());
                });
            } else {
                ui.small("This module published shared state without a human summary.");
            }

            if !shared_state.payload.is_null() {
                let mut payload =
                    serde_json::to_string_pretty(&shared_state.payload).unwrap_or_default();
                ScrollArea::vertical()
                    .id_source(format!("module_bridge_shared_state_{}", module.manifest.id))
                    .max_height(140.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut payload)
                                .desired_rows(6)
                                .interactive(false),
                        );
                    });
            }

            let selected_connections = self.selected_network_connection_ids();
            ui.horizontal_wrapped(|ui| {
                if ui.button("Start new shared session").clicked() {
                    self.reset_module_shared_session(&module.manifest.id);
                    self.networking_status = Some(format!(
                        "Reset the shared session for {}.",
                        module.manifest.title
                    ));
                }
                if selected_connections.is_empty() {
                    ui.small(
                        "Select one or more connected classroom devices in Networking to share this module state.",
                    );
                } else if !can_publish_shared_state {
                    ui.small(
                        "This module has not declared `shared_state_publish` support yet.",
                    );
                } else if ui.button("Share to selected peers").clicked() {
                    let prepared = self.prepare_outgoing_module_shared_state(
                        &module.manifest.id,
                        &shared_state,
                    );
                    match serde_json::to_string_pretty(&prepared) {
                    Ok(text) => {
                        self.remember_recoverable_module_shared_state(
                            &module.manifest.id,
                            &prepared,
                            &text,
                        );
                        let label = format!("{} shared state", module.manifest.title);
                        let summary = if prepared.summary.trim().is_empty() {
                            format!(
                                "Shared classroom state for {}",
                                module.manifest.title.trim()
                            )
                        } else {
                            prepared.summary.trim().to_string()
                        };
                        let file_name = format!(
                            "{}_shared_state.json",
                            slugify_filename(&module.manifest.id, "module")
                        );
                        for connection_id in &selected_connections {
                            self.networking.send_artifact(
                                connection_id,
                                "module_shared_state_json",
                                &label,
                                Some(&module.manifest.id),
                                &summary,
                                &file_name,
                                &text,
                            );
                        }
                        self.networking_status = Some(format!(
                            "Shared {} session {} revision {} with {} connected device(s).",
                            module.manifest.title,
                            prepared.session_id,
                            prepared.session_revision,
                            selected_connections.len()
                        ));
                    }
                    Err(err) => {
                        self.networking_status = Some(format!(
                            "Could not serialize shared state for {}: {}",
                            module.manifest.title, err
                        ));
                    }
                    }
                }
            });
            if !can_receive_shared_state {
                ui.small(
                    "This module has not declared `shared_state_receive` support yet, so incoming classroom state will stay queued for later review.",
                );
            }
        } else {
            ui.small(
                "No shared_state.json yet. Add the optional shared-state plug if you want this module to mirror lesson-ready state across the network.",
            );
        }

        if let Some(incoming) =
            self.read_module_bridge_incoming_shared_state(&module.manifest.id, &module.folder)
        {
            ui.add_space(6.0);
            ui.label(RichText::new("Incoming shared state").strong());
            ui.small(format!(
                "Most recent network state came from {} [{}].",
                if incoming.from_device_name.trim().is_empty() {
                    "(unknown device)"
                } else {
                    incoming.from_device_name.trim()
                },
                incoming.from_device_id.trim()
            ));
            if !incoming.session_id.trim().is_empty() {
                ui.small(format!(
                    "Session {} | revision {}{}",
                    incoming.session_id,
                    incoming.session_revision,
                    if incoming.host_authoritative {
                        " | host-authoritative"
                    } else {
                        ""
                    }
                ));
            }
            if !incoming.summary.trim().is_empty() {
                ui.group(|ui| {
                    ui.label(incoming.summary.trim());
                });
            }
            if !incoming.payload.is_null() {
                let mut payload =
                    serde_json::to_string_pretty(&incoming.payload).unwrap_or_default();
                ScrollArea::vertical()
                    .id_source(format!(
                        "module_bridge_incoming_state_{}",
                        module.manifest.id
                    ))
                    .max_height(120.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut payload)
                                .desired_rows(5)
                                .interactive(false),
                        );
                    });
            }
        }

        let incoming_asset_lanes = module
            .manifest
            .network_capabilities
            .as_ref()
            .map(|caps| caps.asset_lanes.clone())
            .unwrap_or_default();
        if !incoming_asset_lanes.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Incoming asset lanes").strong());
            for lane in incoming_asset_lanes {
                let incoming_assets = self.read_module_bridge_incoming_assets(
                    &module.manifest.id,
                    &module.folder,
                    Some(&lane.lane_id),
                );
                ui.group(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(lane.label.trim());
                        ui.small(format!(
                            "[{} | {} waiting]",
                            lane.lane_id,
                            incoming_assets.len()
                        ));
                    });
                    ui.small(format!(
                        "{} | {}{}",
                        lane.direction.label(),
                        lane.delivery_mode.label(),
                        if lane.replayable { " | replayable" } else { "" }
                    ));
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Open lane folder").clicked() {
                            open_path_in_explorer(&bridge_incoming_asset_lane_dir(
                                &module.folder,
                                &lane.lane_id,
                            ));
                        }
                        if !incoming_assets.is_empty() {
                            ui.small("Modules can consume these from the bridge when ready.");
                        }
                    });
                    if incoming_assets.is_empty() {
                        ui.small("No assets are waiting in this lane right now.");
                    } else {
                        for asset in incoming_assets.iter().take(4) {
                            ui.small(format!(
                                "{} | {} | {}",
                                if asset.label.trim().is_empty() {
                                    asset.kind.trim()
                                } else {
                                    asset.label.trim()
                                },
                                if asset.from_device_name.trim().is_empty() {
                                    asset.from_device_id.trim()
                                } else {
                                    asset.from_device_name.trim()
                                },
                                format_network_transfer_meta(
                                    &asset.content_type,
                                    &asset.transfer_encoding,
                                    asset.byte_len,
                                    asset.chunk_count,
                                )
                            ));
                        }
                    }
                    for note in &lane.notes {
                        ui.small(format!("Note: {}", note));
                    }
                });
            }
        }

        let receipts = self.module_session_receipts_for(&module.manifest.id);
        if !receipts.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Recent session apply receipts").strong());
            ScrollArea::vertical()
                .id_source(format!("module_session_receipts_{}", module.manifest.id))
                .max_height(120.0)
                .show(ui, |ui| {
                    for receipt in receipts.iter().take(8) {
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(if receipt.from_device_name.trim().is_empty() {
                                    receipt.from_device_id.trim()
                                } else {
                                    receipt.from_device_name.trim()
                                });
                                ui.small(format!(
                                    "session {} | revision {} | {}",
                                    receipt.session_id,
                                    receipt.session_revision,
                                    if receipt.applied {
                                        "applied"
                                    } else if receipt.stale {
                                        "stale"
                                    } else {
                                        "not applied"
                                    }
                                ));
                            });
                            if !receipt.message.trim().is_empty() {
                                ui.small(receipt.message.trim());
                            }
                        });
                        ui.add_space(4.0);
                    }
                });
        }

        if let Some(log_context) = self.read_module_bridge_log_context(&module.folder) {
            ui.add_space(6.0);
            ui.label(RichText::new("Recent declared module logs").strong());
            let mut preview = log_context;
            ScrollArea::vertical()
                .id_source(format!("module_bridge_logs_{}", module.manifest.id))
                .max_height(180.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut preview)
                            .desired_rows(10)
                            .interactive(false),
                    );
                });
        }
    }

    fn selected_assignment_ref(&self) -> Option<&HomeworkAssignment> {
        let pack = self.current_pack.as_ref()?;
        let unique_assignments = Self::unique_assignments_by_id(pack);
        if let Some(id) = &self.selected_assignment {
            if let Some(found) = unique_assignments.iter().copied().find(|a| &a.id == id) {
                return Some(found);
            }
        }
        unique_assignments.first().copied()
    }

    fn selected_revision_source(&self) -> Option<&RevisionSource> {
        if let Some(key) = &self.selected_revision {
            if let Some(found) = self
                .revision_sources
                .iter()
                .find(|source| &source.revision_key == key)
            {
                return Some(found);
            }
        }
        self.revision_sources.first()
    }

    fn clean_markdown_fences(text: &str) -> String {
        text.lines()
            .filter(|line| !line.trim_start().starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    fn normalize_compare_text(text: &str) -> String {
        Self::clean_markdown_fences(text)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase()
    }

    fn normalize_homework_match_text(text: &str) -> String {
        let mut out = String::new();
        let mut previous_space = false;

        for ch in text.to_ascii_lowercase().chars() {
            let mapped = if ch.is_ascii_alphanumeric()
                || matches!(ch, '+' | '-' | '*' | '/' | '=' | '^' | '.')
            {
                ch
            } else {
                ' '
            };

            if mapped == ' ' {
                if !previous_space {
                    out.push(' ');
                    previous_space = true;
                }
            } else {
                out.push(mapped);
                previous_space = false;
            }
        }

        out.trim().to_string()
    }

    fn homework_match_tokens(text: &str) -> Vec<String> {
        Self::normalize_homework_match_text(text)
            .split_whitespace()
            .map(|token| token.trim_matches('.'))
            .filter(|token| !token.is_empty())
            .map(|token| token.to_string())
            .collect()
    }

    fn is_homework_stopword(token: &str) -> bool {
        matches!(
            token,
            "a" | "an"
                | "and"
                | "are"
                | "be"
                | "by"
                | "for"
                | "from"
                | "how"
                | "i"
                | "in"
                | "is"
                | "it"
                | "me"
                | "my"
                | "of"
                | "on"
                | "or"
                | "please"
                | "question"
                | "solve"
                | "the"
                | "this"
                | "to"
                | "what"
                | "with"
                | "work"
                | "out"
        )
    }

    fn homework_keyword_tokens(tokens: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for token in tokens {
            let keep = token.chars().any(|ch| ch.is_ascii_digit())
                || token
                    .chars()
                    .any(|ch| matches!(ch, '+' | '-' | '*' | '/' | '=' | '^'))
                || token.len() >= 3;
            if keep && !Self::is_homework_stopword(token) && !out.contains(token) {
                out.push(token.clone());
            }
        }
        out
    }

    fn homework_number_tokens(tokens: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for token in tokens {
            if token.chars().any(|ch| ch.is_ascii_digit()) && !out.contains(token) {
                out.push(token.clone());
            }
        }
        out
    }

    fn homework_signature_phrases(normalized_question: &str, tokens: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        if normalized_question.len() >= 6 {
            out.push(normalized_question.to_string());
        }

        let max_window = tokens.len().min(4);
        for window_size in 2..=max_window {
            for window in tokens.windows(window_size) {
                if window.iter().all(|token| Self::is_homework_stopword(token)) {
                    continue;
                }
                let phrase = window.join(" ");
                if phrase.len() >= 8 && !out.contains(&phrase) {
                    out.push(phrase);
                }
            }
        }

        out
    }

    fn is_assignment_metadata_line(text: &str) -> bool {
        let Some((key, _)) = text.split_once(':') else {
            return false;
        };
        let normalized = key
            .trim()
            .to_ascii_lowercase()
            .replace(' ', "_")
            .replace('-', "_");
        matches!(
            normalized.as_str(),
            "assignment"
                | "attachments"
                | "allow_ai_premark"
                | "allow_games"
                | "class"
                | "due"
                | "due_at"
                | "grade"
                | "grade_level"
                | "gradelevel"
                | "max_score"
                | "school"
                | "student_id"
                | "student_name"
                | "subject"
                | "version"
                | "year"
                | "year_group"
                | "year_level"
                | "yeargroup"
                | "yearlevel"
        )
    }

    fn clean_question_candidate(text: &str) -> String {
        text.trim()
            .trim_matches(|ch: char| matches!(ch, '-' | '*' | '+' | ' '))
            .trim_end_matches(':')
            .trim()
            .to_string()
    }

    fn looks_like_question_candidate(text: &str, from_list_item: bool) -> bool {
        let cleaned = Self::clean_question_candidate(text);
        if cleaned.len() < 5 || Self::is_assignment_metadata_line(&cleaned) {
            return false;
        }

        let lower = cleaned.to_ascii_lowercase();
        if lower.starts_with("student name") || lower.starts_with("student id") {
            return false;
        }
        if cleaned.ends_with('?') {
            return true;
        }
        if [
            "add",
            "calculate",
            "compare",
            "complete",
            "convert",
            "describe",
            "determine",
            "draw",
            "evaluate",
            "explain",
            "find",
            "graph",
            "identify",
            "list",
            "plot",
            "show",
            "simplify",
            "solve",
            "sum",
            "what",
            "which",
            "work out",
            "write",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
        {
            return true;
        }

        let has_digit = cleaned.chars().any(|ch| ch.is_ascii_digit());
        let has_math = cleaned
            .chars()
            .any(|ch| matches!(ch, '+' | '-' | '*' | '/' | '=' | '^'));
        if has_digit && has_math {
            return true;
        }

        from_list_item && cleaned.split_whitespace().count() >= 4
    }

    fn extract_question_candidates(text: &str) -> Vec<(Option<usize>, String)> {
        let mut out = Vec::new();

        for raw_line in Self::clean_markdown_fences(text).lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some((number, rest)) = Self::parse_numbered_instruction_line(trimmed) {
                if Self::looks_like_question_candidate(rest, true) {
                    out.push((Some(number), Self::clean_question_candidate(rest)));
                }
                continue;
            }

            let bullet = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| trimmed.strip_prefix("+ "));
            let candidate = bullet.unwrap_or(trimmed);
            if Self::looks_like_question_candidate(candidate, bullet.is_some()) {
                out.push((None, Self::clean_question_candidate(candidate)));
            }
        }

        out
    }

    fn summarize_text(text: &str, max_lines: usize, max_chars: usize) -> String {
        let mut out = String::new();
        for (idx, line) in text.lines().enumerate() {
            if idx >= max_lines {
                break;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line.trim_end());
        }
        let trimmed = out.trim().to_string();
        if trimmed.len() > max_chars {
            let mut shortened = trimmed;
            shortened.truncate(max_chars);
            shortened
        } else {
            trimmed
        }
    }

    fn assignment_printable_display(&self, assignment: &HomeworkAssignment) -> Option<String> {
        let printable = assignment.student_printable_md.as_deref()?;
        let cleaned = Self::clean_markdown_fences(printable);
        if cleaned.is_empty()
            || Self::normalize_compare_text(&cleaned)
                == Self::normalize_compare_text(&assignment.instructions_md)
        {
            None
        } else {
            Some(cleaned)
        }
    }

    fn assignment_resource_warning(&self, assignment: &HomeworkAssignment) -> Option<String> {
        let lower = assignment.instructions_md.to_ascii_lowercase();
        let mentions_extra_resource = [
            "attached",
            "attachment",
            "worksheet",
            "field notes",
            "table",
            "chart",
            "graph",
            "list",
        ]
        .iter()
        .any(|needle| lower.contains(needle));

        if mentions_extra_resource
            && assignment.attachments.is_empty()
            && self.assignment_printable_display(assignment).is_none()
        {
            Some(
                "This task seems to refer to an extra worksheet/list/resource, but none is visible in this pack."
                    .to_string(),
            )
        } else {
            None
        }
    }

    fn render_markdown_card(&self, ui: &mut egui::Ui, title: &str, body: &str) {
        ui.label(RichText::new(title).strong());
        egui::Frame::none()
            .fill(color_from_hex(&self.theme.surface))
            .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::vec2(10.0, 8.0))
            .show(ui, |ui| {
                render_markdown(ui, body);
            });
    }

    fn resolve_assignment_attachment_path(&self, attachment: &str) -> Option<PathBuf> {
        let trimmed = attachment.trim();
        if trimmed.is_empty() {
            return None;
        }

        let as_path = PathBuf::from(trimmed);
        let mut candidates = Vec::new();
        if as_path.is_absolute() {
            candidates.push(as_path.clone());
        }
        candidates.push(self.base_path.join(trimmed));
        candidates.push(
            self.base_path
                .join("homework")
                .join("assigned")
                .join(trimmed),
        );

        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join(trimmed));
            candidates.push(cwd.join("resources").join(trimmed));
        }

        candidates.into_iter().find(|path| path.exists())
    }

    fn load_text_attachment_preview(
        &self,
        attachment: &str,
        max_lines: usize,
        max_chars: usize,
    ) -> Option<String> {
        let path = self.resolve_assignment_attachment_path(attachment)?;
        let is_text_like = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "txt" | "md" | "markdown" | "csv" | "json" | "log" | "tsv"
                )
            })
            .unwrap_or(true);
        if !is_text_like {
            return None;
        }

        let contents = fs::read_to_string(path).ok()?;
        let preview = Self::summarize_text(&contents, max_lines, max_chars);
        if preview.trim().is_empty() {
            None
        } else {
            Some(preview)
        }
    }

    fn render_assignment_materials(&self, ui: &mut egui::Ui, assignment: &HomeworkAssignment) {
        if let Some(warning) = self.assignment_resource_warning(assignment) {
            ui.colored_label(self.warning_color(), warning);
            ui.add_space(4.0);
        }

        self.render_markdown_card(ui, "Instructions", &assignment.instructions_md);

        if let Some(printable) = self.assignment_printable_display(assignment) {
            ui.add_space(6.0);
            self.render_markdown_card(ui, "Worksheet / handout", &printable);
        }

        if !assignment.attachments.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Attached resources").strong());
            for attachment in &assignment.attachments {
                ui.label(format!("Attachment: {attachment}"));
                if let Some(preview) = self.load_text_attachment_preview(attachment, 16, 1800) {
                    egui::Frame::none()
                        .fill(color_from_hex(&self.theme.surface))
                        .stroke(egui::Stroke::new(1.0, color_from_hex(&self.theme.border)))
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::vec2(10.0, 8.0))
                        .show(ui, |ui| {
                            render_markdown(ui, &preview);
                        });
                } else if self
                    .resolve_assignment_attachment_path(attachment)
                    .is_none()
                {
                    ui.colored_label(
                        self.warning_color(),
                        "Attachment file was not found in the current data/resources folders.",
                    );
                } else {
                    ui.label("Preview unavailable for this attachment type.");
                }
                ui.add_space(4.0);
            }
        }
    }

    fn build_assignment_context(&self, assignment: &HomeworkAssignment) -> String {
        let mut out = format!(
            "Assignment: {} - {}\nSubject: {}\nYear: {}\nDue: {}\nInstructions:\n{}\n",
            assignment.id,
            assignment.title,
            assignment.subject,
            assignment.year_level,
            assignment
                .due_at
                .clone()
                .unwrap_or_else(|| "not set".to_string()),
            assignment.instructions_md.trim()
        );

        if let Some(printable) = self.assignment_printable_display(assignment) {
            out.push_str("\nStudent worksheet / handout:\n");
            out.push_str(printable.trim());
            out.push('\n');
        }

        if !assignment.attachments.is_empty() {
            out.push_str("\nAttached resources:\n");
            for attachment in &assignment.attachments {
                out.push_str(&format!("- {attachment}\n"));
                if let Some(preview) = self.load_text_attachment_preview(attachment, 12, 900) {
                    out.push_str(preview.trim());
                    out.push('\n');
                }
            }
        }

        if let Some(warning) = self.assignment_resource_warning(assignment) {
            out.push_str("\nResource note:\n");
            out.push_str(&warning);
            out.push('\n');
        }

        out
    }

    fn build_memory_context_block(&self) -> String {
        let memory_jogger = self.memory_jogger_items();
        let mut sections = Vec::new();

        if let Some(thoughts) = self.chatty_thoughts_prompt_block() {
            sections.push(format!("Chatty's thoughts for this session:\n{thoughts}"));
        }

        if !memory_jogger.is_empty() {
            sections.push(format!(
                "Memory jogger loaded at session start:\n- {}",
                memory_jogger.join("\n- ")
            ));
        }

        let shared_lukewarm = self.build_applied_lukewarm_context_block();
        if !shared_lukewarm.trim().is_empty() {
            sections.push(format!(
                "Network-shared luke warm context:\n{}",
                shared_lukewarm.trim()
            ));
        }

        if sections.is_empty() {
            String::new()
        } else {
            format!("{}\n\n", sections.join("\n\n"))
        }
    }

    fn bookkeeper_chat_note(assignment: Option<&HomeworkAssignment>) -> Option<String> {
        assignment.map(|assignment| {
            format!(
                "Homework context: {} ({}) | {} | Year {}",
                assignment.title, assignment.id, assignment.subject, assignment.year_level
            )
        })
    }

    fn extract_question_number(student_question: &str) -> Option<usize> {
        let lower = student_question.to_ascii_lowercase();
        let tokens: Vec<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .collect();

        for window in tokens.windows(2) {
            if matches!(window[0], "question" | "q" | "number") {
                if let Ok(number) = window[1].parse::<usize>() {
                    return Some(number);
                }
            }
        }

        for token in tokens {
            if let Some(rest) = token.strip_prefix('q') {
                if let Ok(number) = rest.parse::<usize>() {
                    return Some(number);
                }
            }
        }

        None
    }

    fn parse_numbered_instruction_line(line: &str) -> Option<(usize, &str)> {
        let trimmed = line.trim();
        let mut digits_end = 0usize;
        for ch in trimmed.chars() {
            if ch.is_ascii_digit() {
                digits_end += ch.len_utf8();
            } else {
                break;
            }
        }
        if digits_end == 0 {
            return None;
        }
        let number = trimmed[..digits_end].parse::<usize>().ok()?;
        let rest = trimmed[digits_end..].trim_start();
        if let Some(stripped) = rest.strip_prefix('.') {
            return Some((number, stripped.trim_start()));
        }
        if let Some(stripped) = rest.strip_prefix(')') {
            return Some((number, stripped.trim_start()));
        }
        None
    }

    fn referenced_instruction<'a>(
        &self,
        assignment: &'a HomeworkAssignment,
        student_question: &str,
    ) -> Option<(usize, &'a str)> {
        let wanted = Self::extract_question_number(student_question)?;
        for line in assignment.instructions_md.lines() {
            if let Some((number, rest)) = Self::parse_numbered_instruction_line(line) {
                if number == wanted {
                    return Some((number, rest));
                }
            }
        }
        None
    }

    fn score_homework_question_match(
        normalized_input: &str,
        input_tokens: &HashSet<String>,
        question: &HomeworkQuestionIntercept,
        mentioned_number: Option<usize>,
    ) -> f32 {
        if normalized_input.is_empty() || input_tokens.is_empty() {
            return 0.0;
        }

        if mentioned_number.is_some() && question.question_number == mentioned_number {
            return 1.0;
        }
        if question.normalized_question.len() >= 8
            && normalized_input.contains(&question.normalized_question)
        {
            return 0.98;
        }
        if normalized_input.len() >= 8 && question.normalized_question.contains(normalized_input) {
            return 0.9;
        }

        let signature_hit = question
            .signature_phrases
            .iter()
            .filter(|phrase| normalized_input.contains(phrase.as_str()))
            .count();
        let has_mathy_question = !question.number_tokens.is_empty()
            || question
                .question_text
                .chars()
                .any(|ch| matches!(ch, '+' | '-' | '*' | '/' | '=' | '^'));
        if signature_hit > 0 && has_mathy_question {
            return 0.95;
        }

        let keyword_hits = question
            .keyword_tokens
            .iter()
            .filter(|token| input_tokens.contains(*token))
            .count();
        let number_hits = question
            .number_tokens
            .iter()
            .filter(|token| {
                input_tokens.contains(*token) || normalized_input.contains(token.as_str())
            })
            .count();
        let token_hits = question
            .tokens
            .iter()
            .filter(|token| input_tokens.contains(*token))
            .count();

        let keyword_ratio = if question.keyword_tokens.is_empty() {
            0.0
        } else {
            keyword_hits as f32 / question.keyword_tokens.len() as f32
        };
        let number_ratio = if question.number_tokens.is_empty() {
            0.0
        } else {
            number_hits as f32 / question.number_tokens.len() as f32
        };
        let token_ratio = token_hits as f32 / question.tokens.len() as f32;

        let mut score = keyword_ratio * 0.55 + number_ratio * 0.25 + token_ratio * 0.20;
        if signature_hit > 0 {
            score = score.max(0.82);
        }
        if keyword_hits >= 2 && number_hits > 0 {
            score += 0.18;
        } else if keyword_hits >= 3 {
            score += 0.12;
        } else if token_hits >= 4 {
            score += 0.08;
        }
        if question.number_tokens.is_empty() && keyword_hits >= 2 && keyword_ratio >= 0.5 {
            score += 0.08;
        }

        score.min(1.0)
    }

    fn active_homework_intercept(
        &self,
        user_input: &str,
        assignment: &HomeworkAssignment,
    ) -> Option<HomeworkQuestionIntercept> {
        let normalized_input = Self::normalize_homework_match_text(user_input);
        if normalized_input.is_empty() {
            return None;
        }

        let input_tokens: HashSet<String> = Self::homework_match_tokens(&normalized_input)
            .into_iter()
            .collect();
        let mentioned_number = Self::extract_question_number(user_input);
        let mut best_match: Option<(&HomeworkQuestionIntercept, f32)> = None;

        for question in self
            .homework_question_index
            .iter()
            .filter(|question| question.assignment_id == assignment.id)
        {
            let score = Self::score_homework_question_match(
                &normalized_input,
                &input_tokens,
                question,
                mentioned_number,
            );
            if score >= 0.46 {
                match best_match {
                    Some((_, best_score)) if best_score >= score => {}
                    _ => best_match = Some((question, score)),
                }
            }
        }

        best_match.map(|(question, _)| question.clone())
    }

    fn homework_override_instruction(question: &HomeworkQuestionIntercept) -> String {
        let label = question
            .question_number
            .map(|number| number.to_string())
            .unwrap_or_else(|| "X".to_string());
        format!(
            "[SYSTEM OVERRIDE: The following message contains or closely resembles question [{label}] from the active homework assignment. You must not provide the answer under any circumstances. Respond with a Socratic hint only — ask a question that points the student in the right direction. Do not state any part of the answer.]\nMatched homework question: {}\n",
            question.question_text
        )
    }

    fn hint_response_looks_like_answer(response: &str) -> bool {
        let lower = response.to_ascii_lowercase();
        if lower.contains("the answer is")
            || lower.contains("so the answer")
            || lower.contains("= ")
            || lower.contains(" equals ")
            || lower.contains("therefore")
            || lower.contains("add them together")
        {
            return true;
        }

        let trimmed = response.trim().trim_end_matches(['.', '!', '?']);
        let last_token = trimmed.split_whitespace().last().unwrap_or_default();
        if !last_token.is_empty()
            && last_token
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '/' | '-'))
        {
            return true;
        }

        false
    }

    fn build_rule_based_hint(
        &self,
        assignment: &HomeworkAssignment,
        student_question: &str,
    ) -> String {
        let lower_question = student_question.to_ascii_lowercase();
        if let Some(warning) = self.assignment_resource_warning(assignment) {
            if [
                "list",
                "attached",
                "attachment",
                "worksheet",
                "table",
                "chart",
            ]
            .iter()
            .any(|needle| lower_question.contains(needle))
            {
                return format!(
                    "I can't give the answer yet because the extra list/resource this task seems to need is not visible here. {}",
                    warning
                );
            }
        }

        if let Some((number, line)) = self.referenced_instruction(assignment, student_question) {
            let lowered = line.to_ascii_lowercase();
            if lowered.contains("list")
                || lowered.contains("attached")
                || lowered.contains("worksheet")
            {
                if let Some(warning) = self.assignment_resource_warning(assignment) {
                    return format!(
                        "I can't give the answer yet because question {} seems to rely on an extra worksheet/list/resource that is not visible here. {}",
                        number, warning
                    );
                }
            }
            let hint = if lowered.contains("sum")
                || lowered.contains("add up")
                || lowered.contains("total")
            {
                "Start by listing exactly the values you need, then add them one step at a time and check that you used each value once."
            } else if lowered.contains("solve for x")
                || lowered.contains("solve for y")
                || lowered.contains("equation")
            {
                "Try isolating the variable by undoing the operations around it in reverse order."
            } else if lowered.contains("area") && lowered.contains("triangle") {
                "Write the triangle area rule first, substitute the base and height, and remember it is half of the matching rectangle area."
            } else if lowered.contains("area") && lowered.contains("rectangle") {
                "Write the rectangle area rule first, then substitute the given length and width before multiplying."
            } else if lowered.contains("perimeter") {
                "Perimeter means the total distance around the outside, so think about which side lengths need to be added."
            } else if lowered.contains("decimal") && lowered.contains('/') {
                "Turn the fraction into a division problem: numerator divided by denominator."
            } else if lowered.contains("volume") && lowered.contains("cube") {
                "A cube has the same side length in all three dimensions, so think about multiplying the side length three times."
            } else if lowered.contains("square root") {
                "Ask yourself which number multiplied by itself gives the target number."
            } else {
                "Write down what the question is asking you to find, underline the useful numbers or facts, and do only the first step."
            };

            return format!(
                "I can't give the answer, but here's a hint for question {}: {}",
                number, hint
            );
        }

        if let Some(warning) = self.assignment_resource_warning(assignment) {
            return format!(
                "I can't give the answer, but I can point out the issue first: {}",
                warning
            );
        }

        "I can't give the answer, but here's a way to think about it: write down what the question is asking you to find, list the information it gives you, and do only the first step.".to_string()
    }

    fn safe_homework_hint_response(
        &self,
        assignment: &HomeworkAssignment,
        student_question: &str,
        model_response: &str,
    ) -> String {
        let cleaned = Self::normalize_model_message(model_response);
        if cleaned.trim().is_empty() || Self::hint_response_looks_like_answer(&cleaned) {
            self.build_rule_based_hint(assignment, student_question)
        } else {
            cleaned
        }
    }

    fn is_homework_related_message(
        &self,
        user_input: &str,
        assignment: &HomeworkAssignment,
    ) -> bool {
        let lower = user_input.to_ascii_lowercase();
        if [
            "homework",
            "assignment",
            "question",
            "worksheet",
            "hint",
            "help",
            "answer",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
        {
            return true;
        }

        lower.contains(&assignment.id.to_ascii_lowercase())
            || lower.contains(&assignment.title.to_ascii_lowercase())
            || lower.contains(&assignment.subject.to_ascii_lowercase())
    }

    fn render_submission_area(&mut self, ui: &mut egui::Ui) {
        ui.label("Type your work and export a submission file to upload via the portal.");
        ui.add(
            egui::TextEdit::multiline(&mut self.submission_text)
                .hint_text("Your answers, notes, or summary..."),
        );
        ui.horizontal(|ui| {
            if ui.button("Add attachments...").clicked() {
                if let Some(files) = FileDialog::new().pick_files() {
                    for f in files {
                        if let Some(p) = f.to_str() {
                            self.submission_attachments.push(p.to_string());
                        }
                    }
                }
            }
            if ui.button("Clear attachments").clicked() {
                self.submission_attachments.clear();
            }
        });
        if !self.submission_attachments.is_empty() {
            ui.label("Attachments:");
            let mut to_remove: Option<usize> = None;
            for (idx, path) in self.submission_attachments.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{path}"));
                    if ui.small_button("x").clicked() {
                        to_remove = Some(idx);
                    }
                });
            }
            if let Some(idx) = to_remove {
                self.submission_attachments.remove(idx);
            }
        }
        let assignment = self.selected_assignment_ref().cloned();
        let disabled = assignment.is_none();
        if ui
            .add_enabled(!disabled, egui::Button::new("Export submission file"))
            .clicked()
        {
            if let Some(assignment) = assignment {
                self.pulse_ecg(52.0, "Writing a student submission file.");
                match save_submission_for_assignment(
                    &self.base_path,
                    &self.settings,
                    &assignment,
                    &self.submission_text,
                    &self.submission_attachments,
                ) {
                    Ok(path) => {
                        let _ = ui.label(format!("Saved to {}", path.display()));
                        self.submission_text.clear();
                        self.submission_attachments.clear();
                        self.resync_homework();
                        self.resync_revision();
                    }
                    Err(e) => {
                        let _ = ui.label(format!("Failed: {e}"));
                    }
                }
            }
        }
    }

    fn submission_rows(&self) -> Vec<SubmissionRow> {
        let mut rows = Vec::new();
        for s in &self.submissions {
            let (title, subject) = self
                .current_pack
                .as_ref()
                .and_then(|p| {
                    p.assignments
                        .iter()
                        .find(|a| a.id == s.assignment_id)
                        .map(|a| (a.title.clone(), a.subject.clone()))
                })
                .unwrap_or_else(|| ("Assignment".to_string(), "General".to_string()));
            let score = s
                .ai_score
                .or(s.score)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let feedback = s
                .ai_feedback
                .clone()
                .unwrap_or_else(|| "No AI feedback".to_string());
            rows.push(SubmissionRow {
                assignment_id: s.assignment_id.clone(),
                assignment_title: title,
                student_id: s.student_id.clone(),
                student_name: s.student_name.clone(),
                subject,
                score,
                feedback,
                submitted_at: s.submitted_at.clone(),
            });
        }
        rows
    }

    fn score_entries(&self) -> Vec<StudentScore> {
        self.submissions
            .iter()
            .map(|s| {
                let subject = self
                    .current_pack
                    .as_ref()
                    .and_then(|p| {
                        p.assignments
                            .iter()
                            .find(|a| a.id == s.assignment_id)
                            .map(|a| a.subject.clone())
                    })
                    .unwrap_or_else(|| "General".to_string());
                let score_val = s.ai_score.or(s.score).unwrap_or(0) as f32;
                StudentScore {
                    student_id: s.student_id.clone(),
                    student_name: s.student_name.clone(),
                    subject,
                    score: score_val,
                }
            })
            .collect()
    }

    fn build_chat_system_prompt(
        &self,
        user_prompt: &str,
        assignment: Option<&HomeworkAssignment>,
        intercept: Option<&HomeworkQuestionIntercept>,
    ) -> String {
        let memory_context = self.build_memory_context_block();
        let recent_chat_context = build_recent_chat_prompt_context(&self.chat_log, 6, 2_400);
        let sandbox_context = build_sandbox_prompt_context(
            self.sandbox_dir.as_deref(),
            DEFAULT_SANDBOX_SCRATCHPAD_REL_PATH,
            DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH,
        );
        let task_ledger_nudge = if self.settings.allow_sandbox_tool_requests {
            build_task_ledger_prompt_nudge(user_prompt, self.sandbox_dir.as_deref())
        } else {
            None
        };
        let sandbox_tool_result = if self.sandbox_last_tool_result.trim().is_empty() {
            None
        } else {
            Some(truncate_for_ui(
                self.sandbox_last_tool_result.trim(),
                12_000,
            ))
        };
        let sandbox_policy = if self.settings.allow_sandbox_tool_requests {
            format!(
                "### SANDBOX TOOL POLICY\n\
You may request sandbox file operations inside `{}/` when they would genuinely help with a multi-step task.\n\
The persistent scratchpad lives at `{}`.\n\
The structured task ledger lives at `{}`.\n\
When you need a file action, output ONLY newline-separated JSON objects like:\n\
{{\"tool\":\"sandbox.read\",\"path\":\"notes/today.md\"}}\n\
{{\"tool\":\"sandbox.append\",\"path\":\"scratchpad/current.md\",\"contents\":\"\\n- new note\"}}\n\
{{\"tool\":\"sandbox.preload\",\"paths\":[\"notes/today.md\"],\"include_list\":true,\"include_scratchpad\":true,\"include_ledger\":true,\"note\":\"gather context first\"}}\n\
{{\"tool\":\"sandbox.ledger\",\"status\":\"active\",\"current_task\":\"...\",\"next_step\":\"...\",\"open_questions\":[\"...\"],\"files_touched\":[\"...\"],\"notes\":[\"...\"]}}\n\
Do not use absolute paths. Stay inside `{}/`.\n",
                crate::sandbox::SANDBOX_DIR_NAME,
                DEFAULT_SANDBOX_SCRATCHPAD_REL_PATH,
                DEFAULT_SANDBOX_TASK_LEDGER_REL_PATH,
                crate::sandbox::SANDBOX_DIR_NAME
            )
        } else {
            "### SANDBOX TOOL POLICY\nSandbox tool requests are disabled. Do not request file operations.\n"
                .to_string()
        };

        let sandbox_blocks = format!(
            "\n### RECENT CHAT CONTEXT\n{}\n\n### SANDBOX CONTEXT\n{}\n",
            if recent_chat_context.trim().is_empty() {
                "(none yet)"
            } else {
                recent_chat_context.trim()
            },
            sandbox_context.trim()
        );
        let last_tool_block = sandbox_tool_result
            .map(|text| format!("\n### LAST SANDBOX TOOL RESULT\n{}\n", text))
            .unwrap_or_default();
        let ledger_nudge_block = task_ledger_nudge
            .map(|text| format!("\n### TASK LEDGER NUDGE\n{}\n", text))
            .unwrap_or_default();
        if let Some(assignment) = assignment {
            let homework_rules = if self.settings.homework_hints_only {
                "Homework-aware mode: if the user's message is about the assignment below, use that context and answer with hints only. Never provide a full solution, final answer, or text that could be submitted. Give a short hint, a few steps, or a guiding question instead."
            } else {
                "Homework-aware mode: if the user's message is about the assignment below, use that context. Prefer helpful tutoring, explanation, and guidance over just giving the final answer. If the request is unrelated to homework, ignore the assignment context and answer normally."
            };
            let override_text = intercept
                .map(Self::homework_override_instruction)
                .unwrap_or_default();

            return format!(
                "{capsule}\n{memory_context}{sandbox_blocks}{last_tool_block}{ledger_nudge_block}{sandbox_policy}{rules}\n{override_text}Current assignment context:\n{context}\nRespond with one short, clear answer. Only use the assignment context if the student's next message appears to be about that homework.",
                capsule = CHAT_CAPSULE,
                memory_context = memory_context,
                sandbox_blocks = sandbox_blocks,
                last_tool_block = last_tool_block,
                ledger_nudge_block = ledger_nudge_block,
                sandbox_policy = sandbox_policy,
                rules = homework_rules,
                override_text = override_text,
                context = self.build_assignment_context(assignment),
            );
        }

        format!(
            "{capsule}\n{memory_context}{sandbox_blocks}{last_tool_block}{ledger_nudge_block}{sandbox_policy}Respond with one short, clear answer.",
            capsule = CHAT_CAPSULE,
            memory_context = memory_context,
            sandbox_blocks = sandbox_blocks,
            last_tool_block = last_tool_block,
            ledger_nudge_block = ledger_nudge_block,
            sandbox_policy = sandbox_policy
        )
    }

    fn generate_raw_chat_output(
        &self,
        user_msg: &str,
        assignment: Option<&HomeworkAssignment>,
        intercept: Option<&HomeworkQuestionIntercept>,
    ) -> String {
        let system_prompt = self.build_chat_system_prompt(user_msg, assignment, intercept);
        let result = panic::catch_unwind({
            let settings = self.settings.clone();
            let user_msg = user_msg.to_string();
            move || generate_answer_with_system_prompt(&settings, &system_prompt, &user_msg)
        });
        match result {
            Ok(text) => text,
            Err(_) => "Sorry, I ran into an error while answering.".to_string(),
        }
    }

    fn finalize_chat_model_output(
        &mut self,
        user_msg: &str,
        raw_output: &str,
        assignment: Option<&HomeworkAssignment>,
        intercept: Option<&HomeworkQuestionIntercept>,
    ) -> String {
        if self.settings.allow_sandbox_tool_requests {
            let mut actions = extract_sandbox_actions_from_text(raw_output);
            if !actions.is_empty() {
                let count = actions.len();
                self.pending_sandbox_actions.clear();
                self.pending_sandbox_actions.append(&mut actions);
                self.sandbox_action_status =
                    format!("Prepared {count} sandbox action(s) for approval.");
                self.sandbox_task_nudge =
                    build_task_ledger_user_hint(user_msg, self.sandbox_dir.as_deref())
                        .unwrap_or_default();
                return format!(
                    "I prepared {count} sandbox action(s) for approval so I can work inside {}/ safely. Review them below, or use Preload + Continue if you'd like me to gather context first.",
                    crate::sandbox::SANDBOX_DIR_NAME
                );
            }
        }

        if let Some(assignment) = assignment {
            if intercept.is_some()
                || (self.settings.homework_hints_only
                    && self.is_homework_related_message(user_msg, assignment))
            {
                self.safe_homework_hint_response(assignment, user_msg, raw_output)
            } else {
                Self::normalize_model_message(raw_output)
            }
        } else {
            Self::normalize_model_message(raw_output)
        }
    }

    fn continue_chat_after_sandbox(&mut self, user_msg: &str) {
        if !self.shared_chat_local_ai_allowed() {
            self.networking_status =
                Some("Shared room policy left AI off for this local turn.".to_string());
            return;
        }
        let selected_assignment = self.selected_assignment_ref().cloned();
        let homework_intercept = selected_assignment
            .as_ref()
            .and_then(|assignment| self.active_homework_intercept(user_msg, assignment));
        let raw_output = self.generate_raw_chat_output(
            user_msg,
            selected_assignment.as_ref(),
            homework_intercept.as_ref(),
        );
        let chat_response = self.finalize_chat_model_output(
            user_msg,
            &raw_output,
            selected_assignment.as_ref(),
            homework_intercept.as_ref(),
        );
        self.chat_log
            .push(("Chatty".to_string(), chat_response.clone()));
        if let Some(bookkeeper) = &self.bookkeeper {
            bookkeeper.append_event(
                "sandbox",
                "Chatty-EDU",
                &chat_response,
                Some("Continued after sandbox context".to_string()),
            );
        }
    }

    fn handle_chat_send(&mut self) {
        if self.chat_input.trim().is_empty() {
            return;
        }
        if let Err(reason) = self.shared_chat_can_send_mirrored_main_chat_message() {
            self.networking_status = Some(format!("Shared room: {reason}"));
            return;
        }
        let user_msg = self.chat_input.trim().to_string();
        self.sandbox_task_nudge =
            build_task_ledger_user_hint(&user_msg, self.sandbox_dir.as_deref()).unwrap_or_default();
        let selected_assignment = self.selected_assignment_ref().cloned();
        let homework_intercept = selected_assignment
            .as_ref()
            .and_then(|assignment| self.active_homework_intercept(&user_msg, assignment));
        let bookkeeper_note = Self::bookkeeper_chat_note(selected_assignment.as_ref());
        self.pulse_ecg(88.0, "Generating a chat response with the local model.");
        self.chat_log.push(("You".to_string(), user_msg.clone()));
        if let Some(bookkeeper) = self.bookkeeper.as_ref() {
            bookkeeper.append_chat_entry("You", &user_msg, bookkeeper_note.clone());
        }
        if self.networking_shared_chat_mirror_main_chat {
            self.broadcast_shared_chat_message("user", "You", &user_msg);
        }
        // Show a placeholder before generation to avoid disappearing messages
        self.chat_log
            .push(("Chatty".to_string(), "...".to_string()));

        if !self.shared_chat_local_ai_allowed() {
            if let Some(last) = self.chat_log.last_mut() {
                last.1 =
                    "AI reply skipped locally because the shared room policy has AI turned off for this turn."
                        .to_string();
            }
            self.chat_input.clear();
            self.networking_status =
                Some("Shared room policy left AI off for this local turn.".to_string());
            return;
        }

        let raw_output = self.generate_raw_chat_output(
            &user_msg,
            selected_assignment.as_ref(),
            homework_intercept.as_ref(),
        );
        let chat_response = self.finalize_chat_model_output(
            &user_msg,
            &raw_output,
            selected_assignment.as_ref(),
            homework_intercept.as_ref(),
        );

        if let Some(last) = self.chat_log.last_mut() {
            last.1 = chat_response.clone();
        }
        if let Some(bookkeeper) = self.bookkeeper.as_ref() {
            bookkeeper.append_chat_entry("Chatty", &chat_response, bookkeeper_note);
        }
        if self.networking_shared_chat_mirror_main_chat {
            self.broadcast_shared_chat_message("assistant", "Chatty-EDU", &chat_response);
        }
        self.chat_input.clear();
    }

    fn render_fmi_about(&self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("About Fractal Media Infrastructure")
            .id_source("chatty_edu_fmi_about")
            .show(ui, |ui| {
                ui.label(
                    "Chatty-EDU is stewarded within Fractal Media Infrastructure as a local-first learning tool.",
                );
                ui.small("Steward: Fractal Media Infrastructure (FMI)");
                ui.small("Site: instance001.github.io");
                ui.small("License: AGPL-3.0-or-later");
                ui.small(
                    "Focus: offline-first educational tooling, portable classroom workflows, and user-owned local operation.",
                );
            });
    }

    fn show_startup_splash(&mut self, ctx: &Context) -> bool {
        if !self.startup_splash_active {
            return false;
        }

        let should_dismiss = self.startup_splash_started_at.elapsed() >= FMI_SPLASH_DURATION
            || ctx.input(|input| {
                input.pointer.any_click()
                    || input.key_pressed(egui::Key::Enter)
                    || input.key_pressed(egui::Key::Space)
                    || input.key_pressed(egui::Key::Escape)
            });
        if should_dismiss {
            self.startup_splash_active = false;
            return false;
        }

        ctx.request_repaint_after(Duration::from_millis(16));
        CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(246, 244, 239))
                    .inner_margin(egui::Margin::same(24.0)),
            )
            .show(ctx, |ui| {
                ui.with_layout(
                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    |ui| {
                        ui.vertical_centered(|ui| {
                            if let Some(texture) = &self.startup_splash_texture {
                                let size = texture.size_vec2();
                                let max_width = ui.available_width().min(520.0);
                                let scale = if size.x > 0.0 {
                                    (max_width / size.x).min(1.0)
                                } else {
                                    1.0
                                };
                                ui.image((texture.id(), size * scale));
                                ui.add_space(18.0);
                            }
                            ui.heading("Fractal Media Infrastructure");
                            ui.label("Local-first educational tooling and classroom workflows.");
                            ui.small("Press click, Enter, Space, or Esc to continue.");
                        });
                    },
                );
            });
        true
    }
}
fn render_markdown(ui: &mut egui::Ui, text: &str) {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            ui.heading(trimmed.trim_start_matches("# ").trim());
        } else if trimmed.starts_with("## ") {
            ui.label(RichText::new(trimmed.trim_start_matches("## ").trim()).strong());
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            ui.label(format!("* {}", trimmed[2..].trim()));
        } else if trimmed.is_empty() {
            ui.add_space(6.0);
        } else {
            ui.label(trimmed);
        }
    }
}

fn aggregate_scores(entries: &[StudentScore]) -> (f32, Vec<(String, f32)>, Vec<(String, f32)>) {
    let mut per_student: HashMap<String, Vec<f32>> = HashMap::new();
    let mut per_subject: HashMap<String, Vec<f32>> = HashMap::new();
    for e in entries {
        per_student
            .entry(e.student_name.clone())
            .or_default()
            .push(e.score);
        per_subject
            .entry(e.subject.clone())
            .or_default()
            .push(e.score);
    }

    let avg = |vals: &[f32]| -> f32 {
        if vals.is_empty() {
            0.0
        } else {
            vals.iter().copied().sum::<f32>() / vals.len() as f32
        }
    };

    let class_overall = avg(&entries.iter().map(|e| e.score).collect::<Vec<_>>());

    let mut per_student_avg: Vec<(String, f32)> =
        per_student.into_iter().map(|(k, v)| (k, avg(&v))).collect();
    per_student_avg.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut per_subject_avg: Vec<(String, f32)> =
        per_subject.into_iter().map(|(k, v)| (k, avg(&v))).collect();
    per_subject_avg.sort_by(|a, b| a.0.cmp(&b.0));

    (class_overall, per_student_avg, per_subject_avg)
}

fn score_color(score: f32) -> egui::Color32 {
    let t = (score / 100.0).clamp(0.0, 1.0);
    let r = ((1.0 - t) * 255.0) as u8;
    let g = (t * 200.0 + 55.0).min(255.0) as u8;
    egui::Color32::from_rgb(r, g, 64)
}

fn color_from_hex(hex: &str) -> egui::Color32 {
    let h = hex.trim_start_matches('#');
    if h.len() == 6 {
        if let Ok(rgb) = u32::from_str_radix(h, 16) {
            let r = ((rgb >> 16) & 0xFF) as u8;
            let g = ((rgb >> 8) & 0xFF) as u8;
            let b = (rgb & 0xFF) as u8;
            return egui::Color32::from_rgb(r, g, b);
        }
    } else if h.len() == 8 {
        if let Ok(rgba) = u32::from_str_radix(h, 16) {
            let r = ((rgba >> 24) & 0xFF) as u8;
            let g = ((rgba >> 16) & 0xFF) as u8;
            let b = ((rgba >> 8) & 0xFF) as u8;
            let a = (rgba & 0xFF) as u8;
            return egui::Color32::from_rgba_premultiplied(r, g, b, a);
        }
    }
    egui::Color32::GRAY
}

#[derive(Debug, Clone, Default)]
struct GgufMetadata {
    version: u32,
    tensor_count: u64,
    kv_count: u64,
    general_architecture: Option<String>,
    general_name: Option<String>,
    general_file_type: Option<i64>,
    general_quantization_version: Option<i64>,
    tokenizer_model: Option<String>,
    context_length: Option<u64>,
    tensor_ggml_type_max: Option<u32>,
    tensor_ggml_type_unique: Option<usize>,
    tensor_ggml_type_top: Vec<(u32, usize)>,
}

fn gguf_metadata_summary(path: &Path) -> Result<String, String> {
    let meta = read_gguf_metadata(path)?;
    let mut out = String::new();
    out.push_str(&format!("- gguf_version: {}\n", meta.version));
    out.push_str(&format!("- gguf_kv_count: {}\n", meta.kv_count));
    out.push_str(&format!("- gguf_tensor_count: {}\n", meta.tensor_count));
    if let Some(v) = meta.general_architecture.as_deref() {
        out.push_str(&format!("- gguf_architecture: {v}\n"));
    }
    if let Some(v) = meta.general_name.as_deref() {
        out.push_str(&format!("- gguf_general_name: {v}\n"));
    }
    if let Some(v) = meta.general_file_type {
        out.push_str(&format!("- gguf_file_type: {v}\n"));
    }
    if let Some(v) = meta.general_quantization_version {
        out.push_str(&format!("- gguf_quantization_version: {v}\n"));
    }
    if let Some(v) = meta.tokenizer_model.as_deref() {
        out.push_str(&format!("- gguf_tokenizer_model: {v}\n"));
    }
    if let Some(v) = meta.context_length {
        out.push_str(&format!("- gguf_context_length: {v}\n"));
    }
    if let Some(v) = meta.tensor_ggml_type_max {
        out.push_str(&format!("- gguf_tensor_ggml_type_max_id: {v}\n"));
    }
    if let Some(v) = meta.tensor_ggml_type_unique {
        out.push_str(&format!("- gguf_tensor_ggml_type_unique: {v}\n"));
    }
    if !meta.tensor_ggml_type_top.is_empty() {
        let list = meta
            .tensor_ggml_type_top
            .iter()
            .map(|(id, n)| format!("{id}={n}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("- gguf_tensor_ggml_type_top: {list}\n"));
    }
    Ok(out)
}

fn read_gguf_metadata(path: &Path) -> Result<GgufMetadata, String> {
    const MAX_GGUF_STRING_BYTES: u64 = 1_048_576; // 1 MiB
    const MAX_GGUF_ARRAY_LEN: u64 = 1_000_000;
    const MAX_KV_COUNT: u64 = 50_000;
    const MAX_TENSOR_COUNT: u64 = 5_000_000;
    const MAX_DIMS: u32 = 64;

    let mut f = fs::File::open(path)
        .map_err(|e| format!("Could not open GGUF file {}: {e}", path.display()))?;

    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)
        .map_err(|e| format!("Could not read GGUF header {}: {e}", path.display()))?;
    if &magic != b"GGUF" {
        return Err("Missing GGUF magic header".to_string());
    }

    let version = read_u32_le(&mut f)?;
    let tensor_count = read_u64_le(&mut f)?;
    let kv_count = read_u64_le(&mut f)?;

    if kv_count > MAX_KV_COUNT {
        return Err(format!(
            "GGUF kv_count too large to parse safely: {kv_count}"
        ));
    }
    if tensor_count > MAX_TENSOR_COUNT {
        return Err(format!(
            "GGUF tensor_count too large to parse safely: {tensor_count}"
        ));
    }

    let mut meta = GgufMetadata {
        version,
        tensor_count,
        kv_count,
        ..Default::default()
    };

    for _ in 0..kv_count {
        let key = read_gguf_string(&mut f, MAX_GGUF_STRING_BYTES)?;
        let value_type = read_u32_le(&mut f)?;
        match key.as_str() {
            "general.architecture" => {
                if value_type == 8 {
                    meta.general_architecture =
                        Some(read_gguf_string(&mut f, MAX_GGUF_STRING_BYTES)?);
                } else {
                    skip_gguf_value(
                        &mut f,
                        value_type,
                        MAX_GGUF_STRING_BYTES,
                        MAX_GGUF_ARRAY_LEN,
                    )?;
                }
            }
            "general.name" => {
                if value_type == 8 {
                    meta.general_name = Some(read_gguf_string(&mut f, MAX_GGUF_STRING_BYTES)?);
                } else {
                    skip_gguf_value(
                        &mut f,
                        value_type,
                        MAX_GGUF_STRING_BYTES,
                        MAX_GGUF_ARRAY_LEN,
                    )?;
                }
            }
            "general.file_type" => match value_type {
                4 => meta.general_file_type = Some(read_u32_le(&mut f)? as i64),
                5 => meta.general_file_type = Some(read_i32_le(&mut f)? as i64),
                10 => meta.general_file_type = Some(read_u64_le(&mut f)? as i64),
                11 => meta.general_file_type = Some(read_i64_le(&mut f)?),
                _ => skip_gguf_value(
                    &mut f,
                    value_type,
                    MAX_GGUF_STRING_BYTES,
                    MAX_GGUF_ARRAY_LEN,
                )?,
            },
            "general.quantization_version" => match value_type {
                4 => meta.general_quantization_version = Some(read_u32_le(&mut f)? as i64),
                5 => meta.general_quantization_version = Some(read_i32_le(&mut f)? as i64),
                10 => meta.general_quantization_version = Some(read_u64_le(&mut f)? as i64),
                11 => meta.general_quantization_version = Some(read_i64_le(&mut f)?),
                _ => skip_gguf_value(
                    &mut f,
                    value_type,
                    MAX_GGUF_STRING_BYTES,
                    MAX_GGUF_ARRAY_LEN,
                )?,
            },
            "tokenizer.ggml.model" => {
                if value_type == 8 {
                    meta.tokenizer_model = Some(read_gguf_string(&mut f, MAX_GGUF_STRING_BYTES)?);
                } else {
                    skip_gguf_value(
                        &mut f,
                        value_type,
                        MAX_GGUF_STRING_BYTES,
                        MAX_GGUF_ARRAY_LEN,
                    )?;
                }
            }
            k if k.ends_with(".context_length") => match value_type {
                4 => meta.context_length = Some(read_u32_le(&mut f)? as u64),
                5 => meta.context_length = Some(read_i32_le(&mut f)? as u64),
                10 => meta.context_length = Some(read_u64_le(&mut f)?),
                11 => meta.context_length = Some(read_i64_le(&mut f)? as u64),
                _ => skip_gguf_value(
                    &mut f,
                    value_type,
                    MAX_GGUF_STRING_BYTES,
                    MAX_GGUF_ARRAY_LEN,
                )?,
            },
            _ => {
                skip_gguf_value(
                    &mut f,
                    value_type,
                    MAX_GGUF_STRING_BYTES,
                    MAX_GGUF_ARRAY_LEN,
                )?;
            }
        }
    }

    let mut type_counts: HashMap<u32, usize> = HashMap::new();
    let mut max_type: u32 = 0;
    for _ in 0..tensor_count {
        skip_gguf_string(&mut f, MAX_GGUF_STRING_BYTES)?;
        let n_dims = read_u32_le(&mut f)?;
        if n_dims > MAX_DIMS {
            return Err(format!("GGUF tensor has too many dims ({n_dims})"));
        }
        for _ in 0..n_dims {
            let _ = read_u64_le(&mut f)?;
        }
        let t = read_u32_le(&mut f)?;
        let _ = read_u64_le(&mut f)?; // offset
        *type_counts.entry(t).or_insert(0) += 1;
        max_type = max_type.max(t);
    }

    meta.tensor_ggml_type_max = Some(max_type);
    meta.tensor_ggml_type_unique = Some(type_counts.len());

    let mut top: Vec<(u32, usize)> = type_counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top.truncate(10);
    meta.tensor_ggml_type_top = top;

    Ok(meta)
}

fn read_u32_le<R: Read>(mut r: R) -> Result<u32, String> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| format!("I/O error: {e}"))?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64_le<R: Read>(mut r: R) -> Result<u64, String> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)
        .map_err(|e| format!("I/O error: {e}"))?;
    Ok(u64::from_le_bytes(buf))
}

fn read_i32_le<R: Read>(mut r: R) -> Result<i32, String> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| format!("I/O error: {e}"))?;
    Ok(i32::from_le_bytes(buf))
}

fn read_i64_le<R: Read>(mut r: R) -> Result<i64, String> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)
        .map_err(|e| format!("I/O error: {e}"))?;
    Ok(i64::from_le_bytes(buf))
}

fn read_gguf_string<R: Read>(mut r: R, max_bytes: u64) -> Result<String, String> {
    let n = read_u64_le(&mut r)?;
    if n > max_bytes {
        return Err(format!("GGUF string too large: {n} bytes"));
    }
    let mut buf = vec![0u8; n as usize];
    r.read_exact(&mut buf)
        .map_err(|e| format!("I/O error: {e}"))?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn skip_gguf_string<R: Read + Seek>(mut r: R, max_bytes: u64) -> Result<(), String> {
    let n = read_u64_le(&mut r)?;
    if n > max_bytes {
        return Err(format!("GGUF string too large to skip: {n} bytes"));
    }
    skip_bytes(&mut r, n)
}

fn skip_bytes<R: Seek>(r: &mut R, n: u64) -> Result<(), String> {
    if n > i64::MAX as u64 {
        return Err("Seek too large".to_string());
    }
    r.seek(SeekFrom::Current(n as i64))
        .map(|_| ())
        .map_err(|e| format!("I/O seek error: {e}"))
}

fn skip_gguf_value<R: Read + Seek>(
    r: &mut R,
    t: u32,
    max_string_bytes: u64,
    max_array_len: u64,
) -> Result<(), String> {
    match t {
        0 | 1 | 7 => skip_bytes(r, 1),
        2 | 3 => skip_bytes(r, 2),
        4 | 5 | 6 => skip_bytes(r, 4),
        10 | 11 | 12 => skip_bytes(r, 8),
        8 => skip_gguf_string(r, max_string_bytes),
        9 => {
            let et = read_u32_le(&mut *r)?;
            let n = read_u64_le(&mut *r)?;
            if n > max_array_len {
                return Err(format!(
                    "GGUF array too large to skip safely: n={n}, et={et}"
                ));
            }
            for _ in 0..n {
                skip_gguf_value(r, et, max_string_bytes, max_array_len)?;
            }
            Ok(())
        }
        _ => Err(format!("Unknown GGUF value type id: {t}")),
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

fn slugify_filename(text: &str, fallback: &str) -> String {
    let ascii = deunicode(text);
    let mut out = String::new();
    let mut previous_sep = false;

    for ch in ascii.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };

        if mapped == '_' {
            if !previous_sep && !out.is_empty() {
                out.push('_');
            }
            previous_sep = true;
        } else {
            out.push(mapped);
            previous_sep = false;
        }
    }

    let slug = out.trim_matches('_').to_string();
    if slug.is_empty() {
        sanitize_filename_component(fallback)
    } else {
        slug
    }
}

fn sanitize_filename_keep_extension(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "transfer.bin".to_string();
    }
    let path = Path::new(trimmed);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("transfer");
    let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    let safe_stem = slugify_filename(stem, "transfer");
    if ext.trim().is_empty() {
        safe_stem
    } else {
        format!("{}.{}", safe_stem, slugify_filename(ext, "bin"))
    }
}

fn infer_transfer_extension(file_name: &str, content_type: &str, binary: bool) -> String {
    let ext = Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !ext.is_empty() {
        return ext;
    }

    let ct = content_type.to_ascii_lowercase();
    if ct.contains("json") {
        "json".to_string()
    } else if ct.contains("markdown") {
        "md".to_string()
    } else if ct.contains("html") {
        "html".to_string()
    } else if ct.contains("css") {
        "css".to_string()
    } else if ct.contains("javascript") {
        "js".to_string()
    } else if ct.contains("plain") {
        "txt".to_string()
    } else if binary {
        "bin".to_string()
    } else {
        "txt".to_string()
    }
}

fn clip_string_for_preview(text: &str, max_chars: usize) -> String {
    let mut preview = text.trim().to_string();
    if preview.chars().count() <= max_chars {
        return preview;
    }
    preview = preview.chars().take(max_chars).collect::<String>();
    preview.push_str("\n\n... preview truncated ...");
    preview
}

fn unique_path_in_dir(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("transfer");
    let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    for index in 2..1000 {
        let next = if ext.is_empty() {
            dir.join(format!("{stem}_{index}"))
        } else {
            dir.join(format!("{stem}_{index}.{ext}"))
        };
        if !next.exists() {
            return next;
        }
    }
    dir.join(format!(
        "{}_{}.{}",
        slugify_filename(stem, "transfer"),
        Utc::now().timestamp_millis().max(0),
        if ext.is_empty() { "bin" } else { ext }
    ))
}

fn load_received_generic_transfer_inbox(
    base: &Path,
) -> io::Result<Vec<ReceivedGenericTransferInboxItem>> {
    let dir = base.join("network_inbox").join("file_transfers");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        match read_received_generic_transfer_record(&path) {
            Ok(record) => items.push(ReceivedGenericTransferInboxItem { path, record }),
            Err(err) => eprintln!(
                "[network] Skipping unreadable generic transfer {}: {}",
                path.display(),
                err
            ),
        }
    }

    items.sort_by(|a, b| {
        b.record
            .received_at_unix_ms
            .cmp(&a.record.received_at_unix_ms)
    });
    Ok(items)
}

fn read_received_generic_transfer_record(path: &Path) -> io::Result<ReceivedGenericTransferRecord> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("received generic transfer parse error: {err}"),
        )
    })
}

fn load_received_homework_pack_inbox(
    base: &Path,
) -> io::Result<Vec<ReceivedHomeworkPackInboxItem>> {
    let dir = base.join("network_inbox").join("homework_packs");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        match read_received_homework_pack_record(&path) {
            Ok(record) => items.push(ReceivedHomeworkPackInboxItem { path, record }),
            Err(err) => eprintln!(
                "[homework] Skipping unreadable received pack record {}: {}",
                path.display(),
                err
            ),
        }
    }

    items.sort_by(|a, b| {
        b.record
            .received_at_unix_ms
            .cmp(&a.record.received_at_unix_ms)
    });
    Ok(items)
}

fn read_received_homework_pack_record(path: &Path) -> io::Result<ReceivedHomeworkPackRecord> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("received pack parse error: {err}"),
        )
    })
}

fn load_received_revision_pack_inbox(
    base: &Path,
) -> io::Result<Vec<ReceivedRevisionPackInboxItem>> {
    let dir = base.join("network_inbox").join("revision_packs");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        match read_received_revision_pack_record(&path) {
            Ok(record) => items.push(ReceivedRevisionPackInboxItem { path, record }),
            Err(err) => eprintln!(
                "[revision] Skipping unreadable received revision pack {}: {}",
                path.display(),
                err
            ),
        }
    }

    items.sort_by(|a, b| {
        b.record
            .received_at_unix_ms
            .cmp(&a.record.received_at_unix_ms)
    });
    Ok(items)
}

fn read_received_revision_pack_record(path: &Path) -> io::Result<ReceivedRevisionPackRecord> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("received revision pack parse error: {err}"),
        )
    })
}

fn load_received_workflow_bundle_inbox(
    base: &Path,
) -> io::Result<Vec<ReceivedWorkflowBundleInboxItem>> {
    let dir = base.join("network_inbox").join("workflow_bundles");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        match read_received_workflow_bundle_record(&path) {
            Ok(record) => items.push(ReceivedWorkflowBundleInboxItem { path, record }),
            Err(err) => eprintln!(
                "[bundle] Skipping unreadable received workflow bundle {}: {}",
                path.display(),
                err
            ),
        }
    }

    items.sort_by(|a, b| {
        b.record
            .received_at_unix_ms
            .cmp(&a.record.received_at_unix_ms)
    });
    Ok(items)
}

fn load_received_lukewarm_inbox(base: &Path) -> io::Result<Vec<ReceivedLukewarmContextInboxItem>> {
    let dir = base.join("network_inbox").join("lukewarm_context");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        match read_received_lukewarm_record(&path) {
            Ok(record) => items.push(ReceivedLukewarmContextInboxItem { path, record }),
            Err(err) => eprintln!(
                "[lukewarm] Skipping unreadable received luke warm context {}: {}",
                path.display(),
                err
            ),
        }
    }

    items.sort_by(|a, b| {
        b.record
            .received_at_unix_ms
            .cmp(&a.record.received_at_unix_ms)
    });
    Ok(items)
}

fn load_applied_lukewarm_contexts(
    base: &Path,
) -> io::Result<Vec<ReceivedLukewarmContextInboxItem>> {
    let dir = base.join("network_inbox").join("applied_lukewarm_context");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        match read_received_lukewarm_record(&path) {
            Ok(record) => items.push(ReceivedLukewarmContextInboxItem { path, record }),
            Err(err) => eprintln!(
                "[lukewarm] Skipping unreadable applied luke warm context {}: {}",
                path.display(),
                err
            ),
        }
    }

    items.sort_by(|a, b| {
        b.record
            .received_at_unix_ms
            .cmp(&a.record.received_at_unix_ms)
    });
    Ok(items)
}

fn read_received_lukewarm_record(path: &Path) -> io::Result<ReceivedLukewarmContextRecord> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("received luke warm context parse error: {err}"),
        )
    })
}

fn read_received_workflow_bundle_record(path: &Path) -> io::Result<ReceivedWorkflowBundleRecord> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("received workflow bundle parse error: {err}"),
        )
    })
}

fn find_latest_revision_pack_markdown(base: &Path) -> io::Result<Option<(PathBuf, String)>> {
    let dir = revision_dir(base);
    if !dir.is_dir() {
        return Ok(None);
    }

    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_md = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        let is_revision_pack = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("revision_pack"));
        if !is_md || !is_revision_pack {
            continue;
        }

        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        match &newest {
            Some((_, current)) if *current >= modified => {}
            _ => newest = Some((path, modified)),
        }
    }

    if let Some((path, _)) = newest {
        let text = fs::read_to_string(&path)?;
        Ok(Some((path, text)))
    } else {
        Ok(None)
    }
}

fn open_path_in_explorer(path: &Path) {
    let target = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let _ = Command::new("explorer").arg(target).spawn();
}

impl App for ChattyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        apply_theme(&self.theme, ctx);
        self.ecg_window.tick(Instant::now());
        ctx.request_repaint_after(self.ecg_window.refresh_interval());
        self.module_host_targets.clear();

        if self.show_startup_splash(ctx) {
            return;
        }

        TopBottomPanel::top("menu_bar").show(ctx, |ui| self.render_menu_bar(ctx, ui));
        TopBottomPanel::top("tabs").show(ctx, |ui| self.render_tab_bar(ui));

        let show_homework_chat_bar = matches!(
            self.tabs.get(self.active_tab).map(|tab| &tab.kind),
            Some(TabKind::Home) | Some(TabKind::Chat)
        );
        if show_homework_chat_bar {
            TopBottomPanel::bottom("chat_input").show(ctx, |ui| {
                ui.vertical(|ui| {
                    if self.sandbox_dir.is_some() {
                        ui.horizontal_wrapped(|ui| {
                            ui.small("Sandbox quick access:");
                            if ui.button("Open scratchpad").clicked() {
                                self.open_default_sandbox_scratchpad();
                            }
                            if ui.button("Open ledger").clicked() {
                                self.open_default_sandbox_task_ledger();
                            }
                            let reopen_response = ui.add_enabled(
                                self.sandbox_last_working_path.is_some(),
                                egui::Button::new("Reopen last working file"),
                            );
                            if reopen_response.clicked() {
                                self.reopen_last_sandbox_working_file();
                            }
                            let last_label = self
                                .sandbox_last_working_path
                                .as_ref()
                                .and_then(|path| path.file_name())
                                .and_then(|name| name.to_str())
                                .map(|name| truncate_for_ui(name, 40))
                                .unwrap_or_else(|| "none yet".to_string());
                            ui.small(format!("Last: {last_label}"));
                        });
                    }
                    let live_task_nudge = if self.settings.allow_sandbox_tool_requests {
                        build_task_ledger_user_hint(&self.chat_input, self.sandbox_dir.as_deref())
                    } else {
                        None
                    };
                    if let Some(hint) = live_task_nudge {
                        ui.horizontal_wrapped(|ui| {
                            ui.small(format!("Task hint: {hint}"));
                        });
                    } else if !self.sandbox_task_nudge.trim().is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.small(format!("Task hint: {}", self.sandbox_task_nudge));
                        });
                    }
                    if !self.pending_sandbox_actions.is_empty()
                        && !self.settings.allow_sandbox_tool_requests
                    {
                        self.pending_sandbox_actions.clear();
                    }
                    if !self.pending_sandbox_actions.is_empty() {
                        ui.group(|ui| {
                            ui.label("Pending sandbox actions (requires approval):");
                            for action in &self.pending_sandbox_actions {
                                match action {
                                    SandboxAction::Write { path, .. } => {
                                        ui.label(format!("- write: {path}"));
                                    }
                                    SandboxAction::Append { path, .. } => {
                                        ui.label(format!("- append: {path}"));
                                    }
                                    SandboxAction::Read { path } => {
                                        ui.label(format!("- read: {path}"));
                                    }
                                    SandboxAction::List => {
                                        ui.label("- list");
                                    }
                                    SandboxAction::Preload {
                                        paths,
                                        include_list,
                                        include_scratchpad,
                                        include_ledger,
                                        note,
                                    } => {
                                        let mut parts = Vec::new();
                                        if *include_list {
                                            parts.push("list".to_string());
                                        }
                                        if *include_scratchpad {
                                            parts.push("scratchpad".to_string());
                                        }
                                        if *include_ledger {
                                            parts.push("task ledger".to_string());
                                        }
                                        if !paths.is_empty() {
                                            parts.push(format!("files: {}", paths.join(", ")));
                                        }
                                        if !note.trim().is_empty() {
                                            parts.push(format!("note: {}", note.trim()));
                                        }
                                        ui.label(format!("- preload: {}", parts.join(" | ")));
                                    }
                                    SandboxAction::Ledger {
                                        status,
                                        current_task,
                                        next_step,
                                        open_questions,
                                        files_touched,
                                        ..
                                    } => {
                                        let mut parts = Vec::new();
                                        if !status.trim().is_empty() {
                                            parts.push(format!("status: {}", status.trim()));
                                        }
                                        if !current_task.trim().is_empty() {
                                            parts.push(format!(
                                                "task: {}",
                                                truncate_for_ui(current_task.trim(), 80)
                                            ));
                                        }
                                        if !next_step.trim().is_empty() {
                                            parts.push(format!(
                                                "next: {}",
                                                truncate_for_ui(next_step.trim(), 80)
                                            ));
                                        }
                                        if !open_questions.is_empty() {
                                            parts.push(format!(
                                                "questions: {}",
                                                open_questions.len()
                                            ));
                                        }
                                        if !files_touched.is_empty() {
                                            parts.push(format!(
                                                "files: {}",
                                                files_touched.join(", ")
                                            ));
                                        }
                                        ui.label(format!("- ledger: {}", parts.join(" | ")));
                                    }
                                }
                            }
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Seed ledger from current prompt").clicked() {
                                    self.seed_default_sandbox_task_ledger_from_context();
                                }
                                if ui.button("Defer actions").clicked() {
                                    self.defer_pending_sandbox_actions();
                                }
                                if ui.button("Preload + Continue").clicked() {
                                    self.preload_sandbox_and_continue();
                                }
                                if ui.button("Approve").clicked() {
                                    self.apply_pending_sandbox_actions(false);
                                }
                                if ui.button("Approve + Continue").clicked() {
                                    self.apply_pending_sandbox_actions(true);
                                }
                                if ui.button("Reject").clicked() {
                                    self.pending_sandbox_actions.clear();
                                    self.sandbox_action_status =
                                        "Rejected sandbox actions.".to_string();
                                }
                            });
                        });
                    }
                    if !self.sandbox_action_status.trim().is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.small(format!("Sandbox: {}", self.sandbox_action_status));
                        });
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(
                            &mut self.networking_shared_chat_mirror_main_chat,
                            "Mirror this chat into the shared room",
                        );
                        if self.networking_shared_chat_mirror_main_chat {
                            ui.small(format!("Mode: {}", self.shared_chat_policy_summary()));
                            if !self.shared_chat_local_ai_allowed() {
                                ui.small(
                                    "Local AI reply is currently disabled by the room policy.",
                                );
                            }
                        }
                    });
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.label("Chat:");
                        let input = ui.add(
                            egui::TextEdit::singleline(&mut self.chat_input)
                                .hint_text("Ask or type a command..."),
                        );
                        if input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.handle_chat_send();
                        }
                        if ui.button("Send").clicked() {
                            self.handle_chat_send();
                        }
                    });
                });
            });
        }

        CentralPanel::default().show(ctx, |ui| {
            if let Some(tab) = self.tabs.get(self.active_tab).cloned() {
                match tab.kind {
                    TabKind::Home => self.render_home(ui),
                    TabKind::Chat => self.render_chat(ui),
                    TabKind::Sandbox => self.render_sandbox(ui),
                    TabKind::Bookkeeper => self.render_bookkeeper(ui),
                    TabKind::Networking => self.render_networking(ui),
                    TabKind::Settings => self.render_settings(ui),
                    TabKind::Diagnostics => self.render_diagnostics(ctx, ui),
                    TabKind::Module { .. } => self.render_module_tab(ui, self.active_tab),
                }
            }
        });

        let networking_changed = self.networking.poll();
        self.networking.set_presence(self.build_local_presence());
        let current_shared_chat_peer_keys = self.shared_chat_connected_peer_keys();
        let shared_chat_peer_membership_changed =
            current_shared_chat_peer_keys != self.networking_shared_chat_connected_peer_keys;
        if shared_chat_peer_membership_changed {
            self.networking_shared_chat_connected_peer_keys = current_shared_chat_peer_keys;
        }
        if networking_changed {
            if shared_chat_peer_membership_changed
                && self.networking_shared_chat_policy.session_active
                && self.shared_chat_is_local_host()
                && !self.networking.snapshot().connected_peers.is_empty()
            {
                self.broadcast_shared_chat_policy_with_options("", false, false, false);
            }
            self.process_networking_changes();
        }
        let now = Instant::now();
        if self
            .networking_shared_chat_presence_next_sync_at
            .map(|due| now >= due)
            .unwrap_or(true)
        {
            self.sync_shared_chat_host_presence();
            self.networking_shared_chat_presence_next_sync_at =
                Some(now + Duration::from_millis(900));
        }

        self.sync_module_shared_room_bridge_state();
        self.sync_module_shared_room_events_bridge();
        self.process_module_outgoing_room_events();
        let hosts_need_repaint = self.sync_module_hosts();
        if hosts_need_repaint || networking_changed {
            ctx.request_repaint();
        }
        let networking_live = self
            .tabs
            .get(self.active_tab)
            .map(|tab| matches!(&tab.kind, TabKind::Networking))
            .unwrap_or(false)
            || self.networking.snapshot().available_for_connectivity
            || !self.networking.snapshot().connected_peers.is_empty();
        if networking_live {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }
}

impl Drop for ChattyApp {
    fn drop(&mut self) {
        for (_, mut host) in self.module_hosts.drain() {
            host.force_stop();
        }
        if let Some(bookkeeper) = self.bookkeeper.take() {
            bookkeeper.shutdown_silently();
        }
    }
}

fn format_network_transfer_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    let value = bytes as f64;
    if value >= MIB {
        format!("{:.1} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KiB", value / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn format_network_transfer_meta(
    content_type: &str,
    transfer_encoding: &str,
    byte_len: u64,
    chunk_count: u32,
) -> String {
    let encoding_label = match transfer_encoding.trim() {
        "base64" => "binary",
        "utf8" => "text",
        other if !other.is_empty() => other,
        _ => "text",
    };
    let content_type = if content_type.trim().is_empty() {
        "(unspecified)"
    } else {
        content_type.trim()
    };
    format!(
        "{} | {} | {} chunk(s) | {}",
        format_network_transfer_size(byte_len),
        encoding_label,
        chunk_count.max(1),
        content_type
    )
}

pub fn launch_gui(base_path: PathBuf, settings: Settings) -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Chatty-EDU")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Chatty-EDU",
        native_options,
        Box::new(move |cc| {
            let app =
                ChattyApp::new(cc, base_path.clone(), settings.clone()).unwrap_or_else(|_| {
                    ChattyApp::new(cc, base_path.clone(), settings.clone())
                        .expect("Failed to start app")
                });
            Box::new(app)
        }),
    )
}

fn load_local_png_texture(
    ctx: &Context,
    path: &Path,
    texture_id: &str,
) -> io::Result<TextureHandle> {
    let image = ImageReader::open(path)?
        .decode()
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = image.into_vec();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Ok(ctx.load_texture(
        texture_id.to_string(),
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}
