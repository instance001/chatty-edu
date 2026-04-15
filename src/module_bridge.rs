use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const BRIDGE_DIR_NAME: &str = "bridge";
const BRIDGE_STATUS_FILE: &str = "status.json";
const BRIDGE_LOG_SOURCES_FILE: &str = "log_sources.json";
const BRIDGE_SHARED_STATE_FILE: &str = "shared_state.json";
const BRIDGE_INCOMING_SHARED_STATE_FILE: &str = "incoming_shared_state.json";
const BRIDGE_SHARED_ROOM_STATE_FILE: &str = "shared_room_state.json";
const BRIDGE_SHARED_ROOM_EVENTS_FILE: &str = "shared_room_events.json";
const BRIDGE_OUTGOING_ROOM_EVENTS_FILE: &str = "outgoing_room_events.json";
const BRIDGE_INCOMING_ASSETS_DIR: &str = "incoming_assets";

const DEFAULT_LOG_TAIL_LINES: usize = 80;
const DEFAULT_LOG_TAIL_CHARS: usize = 4000;
const MAX_LOG_TAIL_LINES: usize = 300;
const MAX_LOG_TAIL_CHARS: usize = 12000;
const MAX_LOG_READ_BYTES: usize = 128 * 1024;
const MAX_BRIDGE_ROOM_EVENTS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeStatus {
    #[serde(default)]
    pub module_id: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub snapshot: String,
    #[serde(default = "default_event_type")]
    pub event_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub updated_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeSharedState {
    #[serde(default)]
    pub module_id: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub session_revision: u64,
    #[serde(default)]
    pub authoritative_device_id: String,
    #[serde(default)]
    pub authoritative_device_name: String,
    #[serde(default)]
    pub host_authoritative: bool,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub updated_at_unix_ms: u64,
}

impl ModuleBridgeSharedState {
    pub fn normalize(&mut self) {
        self.module_id = self.module_id.trim().to_string();
        self.summary = self.summary.trim().to_string();
        self.session_id = self.session_id.trim().to_string();
        self.authoritative_device_id = self.authoritative_device_id.trim().to_string();
        self.authoritative_device_name = self.authoritative_device_name.trim().to_string();
        if self.updated_at_unix_ms == 0 {
            self.updated_at_unix_ms = now_unix_ms();
        }
    }

    pub fn has_content(&self) -> bool {
        !self.summary.trim().is_empty() || !self.payload.is_null()
    }

    pub fn content_fingerprint(&self) -> String {
        format!(
            "{}|{}",
            self.summary.trim(),
            serde_json::to_string(&self.payload).unwrap_or_default()
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeIncomingSharedState {
    #[serde(default)]
    pub module_id: String,
    #[serde(default)]
    pub from_device_id: String,
    #[serde(default)]
    pub from_device_name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub session_revision: u64,
    #[serde(default)]
    pub authoritative_device_id: String,
    #[serde(default)]
    pub authoritative_device_name: String,
    #[serde(default)]
    pub host_authoritative: bool,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub received_at_unix_ms: u64,
}

impl ModuleBridgeIncomingSharedState {
    pub fn normalize(&mut self) {
        self.module_id = self.module_id.trim().to_string();
        self.from_device_id = self.from_device_id.trim().to_string();
        self.from_device_name = self.from_device_name.trim().to_string();
        self.summary = self.summary.trim().to_string();
        self.session_id = self.session_id.trim().to_string();
        self.authoritative_device_id = self.authoritative_device_id.trim().to_string();
        self.authoritative_device_name = self.authoritative_device_name.trim().to_string();
        if self.received_at_unix_ms == 0 {
            self.received_at_unix_ms = now_unix_ms();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeIncomingAssetRecord {
    #[serde(default)]
    pub asset_id: String,
    #[serde(default)]
    pub artifact_id: String,
    #[serde(default)]
    pub module_id: String,
    #[serde(default)]
    pub lane_id: String,
    #[serde(default)]
    pub lane_label: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub transfer_encoding: String,
    #[serde(default)]
    pub byte_len: u64,
    #[serde(default)]
    pub chunk_count: u32,
    #[serde(default)]
    pub binary: bool,
    #[serde(default)]
    pub from_device_id: String,
    #[serde(default)]
    pub from_device_name: String,
    #[serde(default)]
    pub delivered_at_unix_ms: u64,
    #[serde(default)]
    pub payload_file_name: String,
}

impl ModuleBridgeIncomingAssetRecord {
    pub fn normalize(&mut self) {
        self.asset_id = self.asset_id.trim().to_string();
        self.artifact_id = self.artifact_id.trim().to_string();
        self.module_id = self.module_id.trim().to_string();
        self.lane_id = sanitize_lane_id(&self.lane_id);
        self.lane_label = self.lane_label.trim().to_string();
        self.kind = self.kind.trim().to_string();
        self.label = self.label.trim().to_string();
        self.summary = self.summary.trim().to_string();
        self.file_name = self.file_name.trim().to_string();
        self.content_type = self.content_type.trim().to_string();
        self.transfer_encoding = self.transfer_encoding.trim().to_string();
        self.from_device_id = self.from_device_id.trim().to_string();
        self.from_device_name = self.from_device_name.trim().to_string();
        self.payload_file_name = sanitize_filename_component(
            &self.payload_file_name,
            default_payload_file_name(&self.file_name, self.binary),
        );
        if self.delivered_at_unix_ms == 0 {
            self.delivered_at_unix_ms = now_unix_ms();
        }
        if self.asset_id.is_empty() {
            self.asset_id = default_asset_id(
                &self.artifact_id,
                &self.lane_id,
                self.delivered_at_unix_ms,
                &self.payload_file_name,
            );
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeSharedRoomParticipant {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub is_local: bool,
    #[serde(default = "default_true")]
    pub connected: bool,
}

impl ModuleBridgeSharedRoomParticipant {
    pub fn normalize(&mut self) {
        self.device_id = self.device_id.trim().to_string();
        self.device_name = self.device_name.trim().to_string();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeSharedRoomState {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub source_app: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub scope_kind: String,
    #[serde(default)]
    pub scope_module_id: String,
    #[serde(default)]
    pub scope_module_name: String,
    #[serde(default)]
    pub scope_multiplayer: bool,
    #[serde(default)]
    pub active_for_module: bool,
    #[serde(default)]
    pub session_active: bool,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub session_revision: u64,
    #[serde(default)]
    pub session_label: String,
    #[serde(default)]
    pub host_authoritative: bool,
    #[serde(default)]
    pub turn_mode: String,
    #[serde(default)]
    pub ai_mode: String,
    #[serde(default)]
    pub teacher_override: bool,
    #[serde(default)]
    pub host_device_id: String,
    #[serde(default)]
    pub host_device_name: String,
    #[serde(default)]
    pub turn_holder_device_id: String,
    #[serde(default)]
    pub turn_holder_device_name: String,
    #[serde(default)]
    pub connected_peer_count: usize,
    #[serde(default)]
    pub participant_count: usize,
    #[serde(default)]
    pub local_device_id: String,
    #[serde(default)]
    pub local_device_name: String,
    #[serde(default)]
    pub local_is_host: bool,
    #[serde(default)]
    pub local_has_turn: bool,
    #[serde(default)]
    pub host_activity_state: String,
    #[serde(default)]
    pub host_activity_label: String,
    #[serde(default)]
    pub host_activity_updated_at_unix_ms: u64,
    #[serde(default)]
    pub participants: Vec<ModuleBridgeSharedRoomParticipant>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub updated_at_unix_ms: u64,
}

impl ModuleBridgeSharedRoomState {
    pub fn normalize(&mut self) {
        self.version = self.version.trim().to_string();
        if self.version.is_empty() {
            self.version = "1".to_string();
        }
        self.source_app = self.source_app.trim().to_string();
        self.label = self.label.trim().to_string();
        self.scope_kind = self.scope_kind.trim().to_string();
        self.scope_module_id = self.scope_module_id.trim().to_string();
        self.scope_module_name = self.scope_module_name.trim().to_string();
        self.turn_mode = self.turn_mode.trim().to_string();
        self.ai_mode = self.ai_mode.trim().to_string();
        self.session_id = self.session_id.trim().to_string();
        self.session_label = self.session_label.trim().to_string();
        self.host_device_id = self.host_device_id.trim().to_string();
        self.host_device_name = self.host_device_name.trim().to_string();
        self.turn_holder_device_id = self.turn_holder_device_id.trim().to_string();
        self.turn_holder_device_name = self.turn_holder_device_name.trim().to_string();
        self.local_device_id = self.local_device_id.trim().to_string();
        self.local_device_name = self.local_device_name.trim().to_string();
        self.host_activity_state = self.host_activity_state.trim().to_string();
        self.host_activity_label = self.host_activity_label.trim().to_string();
        for participant in &mut self.participants {
            participant.normalize();
        }
        self.participants.retain(|participant| {
            !participant.device_id.is_empty() || !participant.device_name.is_empty()
        });
        self.participant_count = self.participant_count.max(self.participants.len());
        self.summary = self.summary.trim().to_string();
        if self.updated_at_unix_ms == 0 {
            self.updated_at_unix_ms = now_unix_ms();
        }
    }

    pub fn fingerprint(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.updated_at_unix_ms,
            self.scope_kind,
            self.scope_module_id,
            self.scope_module_name,
            self.scope_multiplayer,
            self.active_for_module,
            self.session_active,
            self.session_id,
            self.session_revision,
            self.turn_mode,
            self.ai_mode,
            self.host_activity_state,
            self.host_activity_label,
            self.host_activity_updated_at_unix_ms
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeRoomEvent {
    #[serde(default)]
    pub event_id: String,
    #[serde(default)]
    pub source_app: String,
    #[serde(default)]
    pub scope_module_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub payload_text: String,
    #[serde(default)]
    pub from_device_id: String,
    #[serde(default)]
    pub from_device_name: String,
    #[serde(default)]
    pub local_echo: bool,
    #[serde(default)]
    pub sent_at_unix_ms: u64,
    #[serde(default)]
    pub received_at_unix_ms: u64,
}

impl ModuleBridgeRoomEvent {
    pub fn normalize(&mut self) {
        self.event_id = self.event_id.trim().to_string();
        self.source_app = self.source_app.trim().to_string();
        self.scope_module_id = self.scope_module_id.trim().to_string();
        self.session_id = self.session_id.trim().to_string();
        self.event_type = self.event_type.trim().to_string();
        self.label = self.label.trim().to_string();
        self.content_type = self.content_type.trim().to_string();
        if self.content_type.is_empty() {
            self.content_type = "text/plain; charset=utf-8".to_string();
        }
        self.payload_text = self.payload_text.trim().to_string();
        self.from_device_id = self.from_device_id.trim().to_string();
        self.from_device_name = self.from_device_name.trim().to_string();
        if self.sent_at_unix_ms == 0 {
            self.sent_at_unix_ms = now_unix_ms();
        }
        if self.received_at_unix_ms == 0 {
            self.received_at_unix_ms = self.sent_at_unix_ms;
        }
    }

    pub fn has_content(&self) -> bool {
        !self.event_type.is_empty()
            || !self.label.is_empty()
            || !self.payload_text.is_empty()
            || !self.event_id.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeSharedRoomEvents {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub source_app: String,
    #[serde(default)]
    pub scope_module_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub session_revision: u64,
    #[serde(default)]
    pub updated_at_unix_ms: u64,
    #[serde(default)]
    pub events: Vec<ModuleBridgeRoomEvent>,
}

impl ModuleBridgeSharedRoomEvents {
    pub fn normalize(&mut self) {
        self.version = self.version.trim().to_string();
        if self.version.is_empty() {
            self.version = "1".to_string();
        }
        self.source_app = self.source_app.trim().to_string();
        self.scope_module_id = self.scope_module_id.trim().to_string();
        self.session_id = self.session_id.trim().to_string();
        for event in &mut self.events {
            event.normalize();
        }
        self.events.retain(ModuleBridgeRoomEvent::has_content);
        if self.events.len() > MAX_BRIDGE_ROOM_EVENTS {
            let start = self.events.len() - MAX_BRIDGE_ROOM_EVENTS;
            self.events.drain(0..start);
        }
        if self.updated_at_unix_ms == 0 {
            self.updated_at_unix_ms = now_unix_ms();
        }
    }

    pub fn has_content(&self) -> bool {
        !self.events.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeOutgoingRoomEvent {
    #[serde(default)]
    pub event_id: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub payload_text: String,
    #[serde(default)]
    pub created_at_unix_ms: u64,
}

impl ModuleBridgeOutgoingRoomEvent {
    pub fn normalize(&mut self) {
        self.event_id = self.event_id.trim().to_string();
        self.event_type = self.event_type.trim().to_string();
        self.label = self.label.trim().to_string();
        self.content_type = self.content_type.trim().to_string();
        if self.content_type.is_empty() {
            self.content_type = "text/plain; charset=utf-8".to_string();
        }
        self.payload_text = self.payload_text.trim().to_string();
        if self.created_at_unix_ms == 0 {
            self.created_at_unix_ms = now_unix_ms();
        }
    }

    pub fn has_content(&self) -> bool {
        !self.event_type.is_empty() || !self.label.is_empty() || !self.payload_text.is_empty()
    }
}

impl ModuleBridgeStatus {
    pub fn normalize(&mut self) {
        self.module_id = self.module_id.trim().to_string();
        self.summary = self.summary.trim().to_string();
        self.snapshot = self.snapshot.trim().to_string();
        self.event_type = if self.event_type.trim().is_empty() {
            default_event_type()
        } else {
            self.event_type.trim().to_string()
        };
        self.tags = self
            .tags
            .drain(..)
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
        if self.updated_at_unix_ms == 0 {
            self.updated_at_unix_ms = now_unix_ms();
        }
    }

    pub fn has_content(&self) -> bool {
        !self.summary.trim().is_empty()
            || !self.snapshot.trim().is_empty()
            || !self.payload.is_null()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeLogSource {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub format: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub tail_lines: usize,
    #[serde(default)]
    pub tail_chars: usize,
}

impl ModuleBridgeLogSource {
    pub fn normalize(&mut self) {
        self.path = self.path.trim().replace('\\', "/");
        self.label = self.label.trim().to_string();
        self.format = self.format.trim().to_ascii_lowercase();
        if self.tail_lines == 0 {
            self.tail_lines = DEFAULT_LOG_TAIL_LINES;
        }
        if self.tail_chars == 0 {
            self.tail_chars = DEFAULT_LOG_TAIL_CHARS;
        }
        self.tail_lines = self.tail_lines.clamp(1, MAX_LOG_TAIL_LINES);
        self.tail_chars = self.tail_chars.clamp(200, MAX_LOG_TAIL_CHARS);
    }

    pub fn display_name(&self) -> &str {
        if self.label.trim().is_empty() {
            self.path.trim()
        } else {
            self.label.trim()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleBridgeLogSources {
    #[serde(default)]
    pub sources: Vec<ModuleBridgeLogSource>,
}

#[derive(Debug, Clone)]
pub struct ModuleBridgeLogExcerpt {
    pub path: String,
    pub label: String,
    pub format: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ModuleBridgeLogSourcesFile {
    Wrapped { sources: Vec<ModuleBridgeLogSource> },
    Bare(Vec<ModuleBridgeLogSource>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ModuleBridgeOutgoingRoomEventsFile {
    Wrapped {
        events: Vec<ModuleBridgeOutgoingRoomEvent>,
    },
    Bare(Vec<ModuleBridgeOutgoingRoomEvent>),
}

pub fn bridge_dir(module_dir: &Path) -> PathBuf {
    module_dir.join(BRIDGE_DIR_NAME)
}

pub fn bridge_status_path(module_dir: &Path) -> PathBuf {
    bridge_dir(module_dir).join(BRIDGE_STATUS_FILE)
}

pub fn bridge_log_sources_path(module_dir: &Path) -> PathBuf {
    bridge_dir(module_dir).join(BRIDGE_LOG_SOURCES_FILE)
}

pub fn bridge_shared_state_path(module_dir: &Path) -> PathBuf {
    bridge_dir(module_dir).join(BRIDGE_SHARED_STATE_FILE)
}

pub fn bridge_incoming_shared_state_path(module_dir: &Path) -> PathBuf {
    bridge_dir(module_dir).join(BRIDGE_INCOMING_SHARED_STATE_FILE)
}

pub fn bridge_shared_room_state_path(module_dir: &Path) -> PathBuf {
    bridge_dir(module_dir).join(BRIDGE_SHARED_ROOM_STATE_FILE)
}

pub fn bridge_shared_room_events_path(module_dir: &Path) -> PathBuf {
    bridge_dir(module_dir).join(BRIDGE_SHARED_ROOM_EVENTS_FILE)
}

pub fn bridge_outgoing_room_events_path(module_dir: &Path) -> PathBuf {
    bridge_dir(module_dir).join(BRIDGE_OUTGOING_ROOM_EVENTS_FILE)
}

pub fn bridge_incoming_assets_dir(module_dir: &Path) -> PathBuf {
    bridge_dir(module_dir).join(BRIDGE_INCOMING_ASSETS_DIR)
}

pub fn bridge_incoming_asset_lane_dir(module_dir: &Path, lane_id: &str) -> PathBuf {
    bridge_incoming_assets_dir(module_dir).join(sanitize_lane_id(lane_id))
}

pub fn ensure_bridge_dir(module_dir: &Path) -> Result<PathBuf> {
    let dir = bridge_dir(module_dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("mkdir {}", dir.display()))?;
    Ok(dir)
}

pub fn read_bridge_status(module_dir: &Path) -> Result<Option<ModuleBridgeStatus>> {
    let path = bridge_status_path(module_dir);
    if !path.is_file() {
        return Ok(None);
    }

    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let mut status: ModuleBridgeStatus =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    status.normalize();
    if !status.has_content() {
        return Ok(None);
    }
    Ok(Some(status))
}

pub fn read_bridge_shared_state(module_dir: &Path) -> Result<Option<ModuleBridgeSharedState>> {
    let path = bridge_shared_state_path(module_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let mut state: ModuleBridgeSharedState =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    state.normalize();
    if !state.has_content() {
        return Ok(None);
    }
    Ok(Some(state))
}

pub fn read_bridge_incoming_shared_state(
    module_dir: &Path,
) -> Result<Option<ModuleBridgeIncomingSharedState>> {
    let path = bridge_incoming_shared_state_path(module_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let mut state: ModuleBridgeIncomingSharedState =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    state.normalize();
    Ok(Some(state))
}

pub fn read_bridge_shared_room_state(
    module_dir: &Path,
) -> Result<Option<ModuleBridgeSharedRoomState>> {
    let path = bridge_shared_room_state_path(module_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let mut state: ModuleBridgeSharedRoomState =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    state.normalize();
    Ok(Some(state))
}

pub fn read_bridge_shared_room_events(
    module_dir: &Path,
) -> Result<Option<ModuleBridgeSharedRoomEvents>> {
    let path = bridge_shared_room_events_path(module_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let mut events: ModuleBridgeSharedRoomEvents =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    events.normalize();
    if !events.has_content() {
        return Ok(None);
    }
    Ok(Some(events))
}

pub fn read_bridge_outgoing_room_events(
    module_dir: &Path,
) -> Result<Vec<ModuleBridgeOutgoingRoomEvent>> {
    let path = bridge_outgoing_room_events_path(module_dir);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let parsed: ModuleBridgeOutgoingRoomEventsFile =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    let events = match parsed {
        ModuleBridgeOutgoingRoomEventsFile::Wrapped { events } => events,
        ModuleBridgeOutgoingRoomEventsFile::Bare(events) => events,
    };
    let mut normalized = Vec::new();
    for mut event in events {
        event.normalize();
        if event.has_content() {
            normalized.push(event);
        }
    }
    Ok(normalized)
}

pub fn read_bridge_incoming_assets(
    module_dir: &Path,
    lane_id: Option<&str>,
) -> Result<Vec<ModuleBridgeIncomingAssetRecord>> {
    let mut lane_dirs = Vec::new();
    if let Some(lane_id) = lane_id {
        let lane_dir = bridge_incoming_asset_lane_dir(module_dir, lane_id);
        if lane_dir.is_dir() {
            lane_dirs.push(lane_dir);
        }
    } else {
        let root = bridge_incoming_assets_dir(module_dir);
        if root.is_dir() {
            for entry in
                std::fs::read_dir(&root).with_context(|| format!("read_dir {}", root.display()))?
            {
                let entry = entry.with_context(|| format!("read_dir entry {}", root.display()))?;
                let path = entry.path();
                if path.is_dir() {
                    lane_dirs.push(path);
                }
            }
        }
    }

    let mut records = Vec::new();
    for lane_dir in lane_dirs {
        for entry in std::fs::read_dir(&lane_dir)
            .with_context(|| format!("read_dir {}", lane_dir.display()))?
        {
            let entry = entry.with_context(|| format!("read_dir entry {}", lane_dir.display()))?;
            let path = entry.path();
            if !path.is_file()
                || path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_none_or(|ext| ext != "json")
            {
                continue;
            }
            let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            let mut record: ModuleBridgeIncomingAssetRecord = serde_json::from_slice(&bytes)
                .with_context(|| format!("parse {}", path.display()))?;
            record.normalize();
            if !record.asset_id.is_empty() && !record.lane_id.is_empty() {
                records.push(record);
            }
        }
    }

    records.sort_by(|a, b| b.delivered_at_unix_ms.cmp(&a.delivered_at_unix_ms));
    Ok(records)
}

pub fn write_bridge_status(module_dir: &Path, status: &ModuleBridgeStatus) -> Result<()> {
    let dir = ensure_bridge_dir(module_dir)?;
    let path = dir.join(BRIDGE_STATUS_FILE);
    let mut normalized = status.clone();
    normalized.normalize();

    let bytes = serde_json::to_vec_pretty(&normalized)
        .with_context(|| format!("serialize {}", path.display()))?;
    atomic_write(&path, &bytes)
}

pub fn write_bridge_shared_state(module_dir: &Path, state: &ModuleBridgeSharedState) -> Result<()> {
    let dir = ensure_bridge_dir(module_dir)?;
    let path = dir.join(BRIDGE_SHARED_STATE_FILE);
    let mut normalized = state.clone();
    normalized.normalize();
    let bytes = serde_json::to_vec_pretty(&normalized)
        .with_context(|| format!("serialize {}", path.display()))?;
    atomic_write(&path, &bytes)
}

pub fn write_bridge_incoming_shared_state(
    module_dir: &Path,
    state: &ModuleBridgeIncomingSharedState,
) -> Result<()> {
    let dir = ensure_bridge_dir(module_dir)?;
    let path = dir.join(BRIDGE_INCOMING_SHARED_STATE_FILE);
    let mut normalized = state.clone();
    normalized.normalize();
    let bytes = serde_json::to_vec_pretty(&normalized)
        .with_context(|| format!("serialize {}", path.display()))?;
    atomic_write(&path, &bytes)
}

pub fn write_bridge_shared_room_state(
    module_dir: &Path,
    state: &ModuleBridgeSharedRoomState,
) -> Result<()> {
    let path = bridge_shared_room_state_path(module_dir);
    let mut normalized = state.clone();
    normalized.normalize();
    let bytes = serde_json::to_vec_pretty(&normalized)
        .with_context(|| format!("serialize {}", path.display()))?;
    atomic_write(&path, &bytes)
}

pub fn write_bridge_shared_room_events(
    module_dir: &Path,
    events: &ModuleBridgeSharedRoomEvents,
) -> Result<()> {
    let path = bridge_shared_room_events_path(module_dir);
    let mut normalized = events.clone();
    normalized.normalize();
    let bytes = serde_json::to_vec_pretty(&normalized)
        .with_context(|| format!("serialize {}", path.display()))?;
    atomic_write(&path, &bytes)
}

pub fn write_bridge_outgoing_room_events(
    module_dir: &Path,
    events: &[ModuleBridgeOutgoingRoomEvent],
) -> Result<()> {
    let path = bridge_outgoing_room_events_path(module_dir);
    let mut normalized = events.to_vec();
    for event in &mut normalized {
        event.normalize();
    }
    normalized.retain(ModuleBridgeOutgoingRoomEvent::has_content);
    let bytes = serde_json::to_vec_pretty(&serde_json::json!({ "events": normalized }))
        .with_context(|| format!("serialize {}", path.display()))?;
    atomic_write(&path, &bytes)
}

pub fn append_bridge_outgoing_room_event(
    module_dir: &Path,
    event: &ModuleBridgeOutgoingRoomEvent,
) -> Result<()> {
    let mut events = read_bridge_outgoing_room_events(module_dir).unwrap_or_default();
    let mut normalized = event.clone();
    normalized.normalize();
    if !normalized.has_content() {
        return Ok(());
    }
    events.push(normalized);
    write_bridge_outgoing_room_events(module_dir, &events)
}

pub fn write_bridge_incoming_asset(
    module_dir: &Path,
    lane_id: &str,
    record: &ModuleBridgeIncomingAssetRecord,
    payload_bytes: &[u8],
) -> Result<PathBuf> {
    let lane_dir = bridge_incoming_asset_lane_dir(module_dir, lane_id);
    std::fs::create_dir_all(&lane_dir).with_context(|| format!("mkdir {}", lane_dir.display()))?;
    let mut normalized = record.clone();
    normalized.lane_id = sanitize_lane_id(lane_id);
    normalized.normalize();
    let payload_path = lane_dir.join(&normalized.payload_file_name);
    std::fs::write(&payload_path, payload_bytes)
        .with_context(|| format!("write {}", payload_path.display()))?;
    let record_path = lane_dir.join(format!("{}.json", normalized.asset_id));
    let bytes = serde_json::to_vec_pretty(&normalized)
        .with_context(|| format!("serialize {}", record_path.display()))?;
    atomic_write(&record_path, &bytes)?;
    Ok(record_path)
}

pub fn remove_bridge_incoming_asset(
    module_dir: &Path,
    lane_id: &str,
    asset_id: &str,
) -> Result<bool> {
    let lane_dir = bridge_incoming_asset_lane_dir(module_dir, lane_id);
    if !lane_dir.is_dir() {
        return Ok(false);
    }
    let asset_id = asset_id.trim();
    if asset_id.is_empty() {
        return Ok(false);
    }
    let record_path = lane_dir.join(format!("{asset_id}.json"));
    if !record_path.is_file() {
        return Ok(false);
    }
    let record_bytes =
        std::fs::read(&record_path).with_context(|| format!("read {}", record_path.display()))?;
    let mut record: ModuleBridgeIncomingAssetRecord = serde_json::from_slice(&record_bytes)
        .with_context(|| format!("parse {}", record_path.display()))?;
    record.normalize();
    if !record.payload_file_name.is_empty() {
        let payload_path = lane_dir.join(&record.payload_file_name);
        if payload_path.is_file() {
            std::fs::remove_file(&payload_path)
                .with_context(|| format!("remove {}", payload_path.display()))?;
        }
    }
    std::fs::remove_file(&record_path)
        .with_context(|| format!("remove {}", record_path.display()))?;
    Ok(true)
}

pub fn clear_bridge_status(module_dir: &Path) -> Result<()> {
    let path = bridge_status_path(module_dir);
    if path.is_file() {
        std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

pub fn clear_bridge_shared_state(module_dir: &Path) -> Result<()> {
    let path = bridge_shared_state_path(module_dir);
    if path.is_file() {
        std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

pub fn clear_bridge_shared_room_state(module_dir: &Path) -> Result<()> {
    let path = bridge_shared_room_state_path(module_dir);
    if path.is_file() {
        std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

pub fn clear_bridge_shared_room_events(module_dir: &Path) -> Result<()> {
    let path = bridge_shared_room_events_path(module_dir);
    if path.is_file() {
        std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

pub fn clear_bridge_outgoing_room_events(module_dir: &Path) -> Result<()> {
    let path = bridge_outgoing_room_events_path(module_dir);
    if path.is_file() {
        std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

pub fn read_bridge_log_sources(module_dir: &Path) -> Result<Vec<ModuleBridgeLogSource>> {
    let path = bridge_log_sources_path(module_dir);
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let parsed: ModuleBridgeLogSourcesFile =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    let sources = match parsed {
        ModuleBridgeLogSourcesFile::Wrapped { sources } => sources,
        ModuleBridgeLogSourcesFile::Bare(sources) => sources,
    };

    let mut normalized = Vec::new();
    for mut source in sources {
        source.normalize();
        if !source.enabled || source.path.trim().is_empty() {
            continue;
        }
        normalized.push(source);
    }
    Ok(normalized)
}

pub fn read_bridge_log_excerpts(module_dir: &Path) -> Result<Vec<ModuleBridgeLogExcerpt>> {
    let sources = read_bridge_log_sources(module_dir)?;
    let mut excerpts = Vec::new();
    for source in sources {
        let Some(path) = resolve_module_owned_file(module_dir, &source.path) else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        let excerpt = read_log_excerpt(&path, &source)
            .with_context(|| format!("read log excerpt {}", path.display()))?;
        if excerpt.trim().is_empty() {
            continue;
        }
        excerpts.push(ModuleBridgeLogExcerpt {
            path: source.path.clone(),
            label: source.display_name().to_string(),
            format: source.format.clone(),
            excerpt,
        });
    }
    Ok(excerpts)
}

pub fn write_bridge_log_sources(
    module_dir: &Path,
    log_sources: &ModuleBridgeLogSources,
) -> Result<()> {
    let dir = ensure_bridge_dir(module_dir)?;
    let path = dir.join(BRIDGE_LOG_SOURCES_FILE);
    let mut normalized = log_sources.clone();
    for source in &mut normalized.sources {
        source.normalize();
    }
    normalized
        .sources
        .retain(|source| source.enabled && !source.path.trim().is_empty());
    let bytes = serde_json::to_vec_pretty(&normalized)
        .with_context(|| format!("serialize {}", path.display()))?;
    atomic_write(&path, &bytes)
}

pub fn bridge_env(module_dir: &Path) -> Result<HashMap<String, String>> {
    let dir = ensure_bridge_dir(module_dir)?;
    let status = dir.join(BRIDGE_STATUS_FILE);
    let mut env = HashMap::new();
    env.insert("CHATTYEDU_HOSTED".to_string(), "1".to_string());
    env.insert(
        "CHATTYEDU_MODULE_DIR".to_string(),
        module_dir.display().to_string(),
    );
    env.insert(
        "CHATTYEDU_BRIDGE_DIR".to_string(),
        dir.display().to_string(),
    );
    env.insert(
        "CHATTYEDU_BRIDGE_STATUS".to_string(),
        status.display().to_string(),
    );
    env.insert(
        "CHATTYEDU_BRIDGE_LOG_SOURCES".to_string(),
        dir.join(BRIDGE_LOG_SOURCES_FILE).display().to_string(),
    );
    env.insert(
        "CHATTYEDU_BRIDGE_SHARED_STATE".to_string(),
        dir.join(BRIDGE_SHARED_STATE_FILE).display().to_string(),
    );
    env.insert(
        "CHATTYEDU_BRIDGE_INCOMING_SHARED_STATE".to_string(),
        dir.join(BRIDGE_INCOMING_SHARED_STATE_FILE)
            .display()
            .to_string(),
    );
    env.insert(
        "CHATTYEDU_BRIDGE_SHARED_ROOM_STATE".to_string(),
        dir.join(BRIDGE_SHARED_ROOM_STATE_FILE)
            .display()
            .to_string(),
    );
    env.insert(
        "CHATTYEDU_BRIDGE_SHARED_ROOM_EVENTS".to_string(),
        dir.join(BRIDGE_SHARED_ROOM_EVENTS_FILE)
            .display()
            .to_string(),
    );
    env.insert(
        "CHATTYEDU_BRIDGE_OUTGOING_ROOM_EVENTS".to_string(),
        dir.join(BRIDGE_OUTGOING_ROOM_EVENTS_FILE)
            .display()
            .to_string(),
    );
    env.insert(
        "CHATTYEDU_BRIDGE_INCOMING_ASSETS_DIR".to_string(),
        dir.join(BRIDGE_INCOMING_ASSETS_DIR).display().to_string(),
    );
    Ok(env)
}

fn resolve_module_owned_file(module_dir: &Path, relative: &str) -> Option<PathBuf> {
    let relative = relative.trim();
    if relative.is_empty() {
        return None;
    }
    let rel_path = Path::new(relative);
    if rel_path.is_absolute() {
        return None;
    }
    for component in rel_path.components() {
        match component {
            std::path::Component::Normal(_) => {}
            _ => return None,
        }
    }
    let candidate = module_dir.join(rel_path);
    if !candidate.exists() {
        return None;
    }
    let base = module_dir.canonicalize().ok()?;
    let resolved = candidate.canonicalize().ok()?;
    if !resolved.starts_with(&base) {
        return None;
    }
    Some(resolved)
}

fn sanitize_lane_id(raw: &str) -> String {
    let mut out = String::new();
    let mut previous_sep = false;
    for ch in raw.trim().chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };
        if mapped == '-' {
            if !previous_sep && !out.is_empty() {
                out.push(mapped);
            }
            previous_sep = true;
        } else {
            out.push(mapped);
            previous_sep = false;
        }
    }
    out.trim_matches('-').to_string()
}

fn sanitize_filename_component(raw: &str, fallback: String) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return fallback;
    }
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        let safe = match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => Some(ch),
            _ => Some('_'),
        };
        if let Some(ch) = safe {
            out.push(ch);
        }
    }
    let sanitized = out.trim_matches('_').trim_matches('.').to_string();
    if sanitized.is_empty() {
        fallback
    } else {
        sanitized
    }
}

fn default_payload_file_name(file_name: &str, binary: bool) -> String {
    if binary {
        sanitize_filename_component(file_name, "incoming_asset.bin".to_string())
    } else {
        sanitize_filename_component(file_name, "incoming_asset.txt".to_string())
    }
}

fn default_asset_id(
    artifact_id: &str,
    lane_id: &str,
    delivered_at_unix_ms: u64,
    payload_file_name: &str,
) -> String {
    let base = sanitize_lane_id(&format!(
        "{}-{}-{}",
        lane_id.trim(),
        artifact_id.trim(),
        payload_file_name.trim()
    ));
    if base.is_empty() {
        format!("asset-{delivered_at_unix_ms}")
    } else {
        format!("{base}-{delivered_at_unix_ms}")
    }
}

fn read_log_excerpt(path: &Path, source: &ModuleBridgeLogSource) -> Result<String> {
    let max_bytes = source
        .tail_chars
        .saturating_mul(6)
        .clamp(4096, MAX_LOG_READ_BYTES);
    let (text, truncated_front) = read_tail_text(path, max_bytes)?;
    let mut lines: Vec<&str> = text.lines().collect();
    if truncated_front && !lines.is_empty() {
        lines.remove(0);
    }
    if lines.len() > source.tail_lines {
        lines = lines[lines.len().saturating_sub(source.tail_lines)..].to_vec();
    }
    let excerpt = lines.join("\n");
    Ok(clamp_chars_ellipsis(excerpt.trim(), source.tail_chars))
}

fn read_tail_text(path: &Path, max_bytes: usize) -> Result<(String, bool)> {
    let meta = std::fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    let total_len = meta.len() as usize;
    let read_len = total_len.min(max_bytes);
    let mut file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let truncated_front = total_len > read_len;
    if truncated_front {
        file.seek(SeekFrom::End(-(read_len as i64)))
            .with_context(|| format!("seek {}", path.display()))?;
    }
    let mut bytes = vec![0_u8; read_len];
    file.read_exact(&mut bytes)
        .with_context(|| format!("read {}", path.display()))?;
    Ok((
        String::from_utf8_lossy(&bytes).replace("\r\n", "\n"),
        truncated_front,
    ))
}

fn clamp_chars_ellipsis(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

fn default_event_type() -> String {
    "suspend_rundown".to_string()
}

fn default_enabled() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn now_unix_ms() -> u64 {
    let now = std::time::SystemTime::now();
    now.duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
