use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

const APP_PROTOCOL: &str = "chatty-edu-lan-v2";
const APP_PROTOCOL_FAMILY: &str = "chatty-edu-lan";
const NETWORK_PROTOCOL_VERSION: u8 = 2;
const MIN_SUPPORTED_PROTOCOL_VERSION: u8 = 2;
const MAX_SUPPORTED_PROTOCOL_VERSION: u8 = 2;
const DISCOVERY_PORT: u16 = 45841;
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(4);
const STATUS_INTERVAL: Duration = Duration::from_secs(2);
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(500);
const PEER_TTL: Duration = Duration::from_secs(15);
const LOOP_SLEEP: Duration = Duration::from_millis(100);
const MAX_RECENT_ARTIFACTS: usize = 24;
const MAX_TRACKED_OUTGOING_ARTIFACTS: usize = 24;
const OUTGOING_ARTIFACT_TTL: Duration = Duration::from_secs(300);
const RECENT_ARTIFACT_ID_TTL: Duration = Duration::from_secs(600);
const PARTIAL_ARTIFACT_TTL: Duration = Duration::from_secs(120);
const ARTIFACT_ACK_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_ARTIFACT_SEND_ATTEMPTS: u32 = 3;
const MAX_ARTIFACT_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_ARTIFACT_CHUNK_BYTES: usize = 64 * 1024;
const MAX_INCOMING_ARTIFACT_ASSEMBLIES: usize = 24;
const MAX_RECENT_SESSION_EVENTS: usize = 64;
const RECENT_SESSION_EVENT_ID_TTL: Duration = Duration::from_secs(180);
const MAX_SESSION_EVENT_TEXT_BYTES: usize = 16 * 1024;

const ARTIFACT_ENCODING_UTF8: &str = "utf8";
const ARTIFACT_ENCODING_BASE64: &str = "base64";
const CONTENT_TYPE_TEXT: &str = "text/plain; charset=utf-8";
const CONTENT_TYPE_JSON: &str = "application/json";
const CONTENT_TYPE_MARKDOWN: &str = "text/markdown; charset=utf-8";
const CONTENT_TYPE_BINARY: &str = "application/octet-stream";

#[derive(Debug, Clone, Default)]
pub struct NetworkSnapshot {
    pub device_id: String,
    pub device_name: String,
    pub available_for_connectivity: bool,
    pub allow_unknown_devices: bool,
    pub listener_port: Option<u16>,
    pub status: String,
    pub protocol_notice: String,
    pub last_error: String,
    pub local_presence: LocalPresence,
    pub discovered_peers: Vec<DiscoveredPeer>,
    pub connected_peers: Vec<ConnectedPeer>,
    pub trusted_peers: Vec<TrustedPeer>,
    pub blocked_peers: Vec<BlockedPeer>,
    pub pending_requests: Vec<PendingPeerRequest>,
    pub received_handoffs: Vec<ReceivedHandoff>,
    pub received_artifacts: Vec<ReceivedArtifact>,
    pub received_session_events: Vec<ReceivedSessionEvent>,
    pub outgoing_artifacts: Vec<OutgoingArtifactDelivery>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalPresence {
    pub active_tab: String,
    pub runtime_status: String,
    pub model_label: String,
    pub is_generating: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DiscoveredPeer {
    pub device_id: String,
    pub device_name: String,
    pub address: String,
    pub host_port: u16,
    pub last_seen_secs_ago: u64,
    pub connected_connection_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ConnectedPeer {
    pub connection_id: String,
    pub device_id: String,
    pub device_name: String,
    pub address: String,
    pub inbound: bool,
    pub connected_secs: u64,
    pub status_summary: String,
    pub status_age_secs: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustedPeer {
    pub device_id: String,
    pub device_name: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub last_seen_secs_ago: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockedPeer {
    pub device_id: String,
    pub device_name: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub last_seen_secs_ago: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct PendingPeerRequest {
    pub device_id: String,
    pub device_name: String,
    pub address: String,
    pub requested_secs_ago: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ReceivedHandoff {
    pub handoff_id: String,
    pub from_device_id: String,
    pub from_device_name: String,
    pub from_address: String,
    pub title: String,
    pub body: String,
    pub received_secs_ago: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ReceivedArtifact {
    pub artifact_id: String,
    pub from_device_id: String,
    pub from_device_name: String,
    pub from_address: String,
    pub kind: String,
    pub label: String,
    pub module_id: String,
    pub summary: String,
    pub file_name: String,
    pub content_type: String,
    pub transfer_encoding: String,
    pub byte_len: u64,
    pub chunk_count: u32,
    pub text: String,
    pub data_base64: String,
    pub received_secs_ago: u64,
}

impl ReceivedArtifact {
    pub fn is_binary(&self) -> bool {
        self.transfer_encoding == ARTIFACT_ENCODING_BASE64 || !self.data_base64.is_empty()
    }

    #[allow(dead_code)]
    pub fn decoded_bytes(&self) -> Option<Vec<u8>> {
        if self.is_binary() {
            BASE64.decode(self.data_base64.as_bytes()).ok()
        } else {
            Some(self.text.as_bytes().to_vec())
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReceivedSessionEvent {
    pub event_id: String,
    pub from_device_id: String,
    pub from_device_name: String,
    pub from_address: String,
    pub scope_module_id: String,
    pub session_id: String,
    pub event_type: String,
    pub label: String,
    pub content_type: String,
    pub payload_text: String,
    pub received_at_unix_ms: u64,
    pub received_secs_ago: u64,
}

#[derive(Debug, Clone, Default)]
pub struct OutgoingArtifactDelivery {
    pub artifact_id: String,
    pub to_device_id: String,
    pub to_device_name: String,
    pub to_address: String,
    pub kind: String,
    pub label: String,
    pub module_id: String,
    pub summary: String,
    pub file_name: String,
    pub content_type: String,
    pub transfer_encoding: String,
    pub byte_len: u64,
    pub chunk_count: u32,
    pub status: String,
    pub attempts: u32,
    pub waiting_for_ack: bool,
    pub updated_secs_ago: u64,
}

#[derive(Debug)]
pub struct NetworkController {
    command_tx: Sender<NetworkCommand>,
    event_rx: Receiver<NetworkEvent>,
    snapshot: NetworkSnapshot,
}

impl NetworkController {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::new_with_identity(None)
    }

    pub fn new_with_identity(device_id: Option<String>) -> Self {
        let (command_tx, command_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        let snapshot = NetworkSnapshot {
            device_id: device_id
                .as_deref()
                .and_then(sanitize_device_id)
                .unwrap_or_else(make_device_id),
            device_name: make_device_name(),
            available_for_connectivity: false,
            allow_unknown_devices: true,
            listener_port: None,
            status: "Networking idle. Turn on availability to host; use Refresh to scan the LAN."
                .to_string(),
            protocol_notice: String::new(),
            last_error: String::new(),
            local_presence: LocalPresence::default(),
            discovered_peers: Vec::new(),
            connected_peers: Vec::new(),
            trusted_peers: Vec::new(),
            blocked_peers: Vec::new(),
            pending_requests: Vec::new(),
            received_handoffs: Vec::new(),
            received_artifacts: Vec::new(),
            received_session_events: Vec::new(),
            outgoing_artifacts: Vec::new(),
        };

        let worker_snapshot = snapshot.clone();
        std::thread::spawn(move || {
            let mut service = NetworkService::new(worker_snapshot, command_rx, event_tx);
            service.run();
        });

        Self {
            command_tx,
            event_rx,
            snapshot,
        }
    }

    pub fn snapshot(&self) -> &NetworkSnapshot {
        &self.snapshot
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                NetworkEvent::Snapshot(snapshot) => {
                    self.snapshot = snapshot;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn set_available(&mut self, enabled: bool) {
        if self.snapshot.available_for_connectivity != enabled {
            self.snapshot.available_for_connectivity = enabled;
            if enabled {
                self.snapshot.status = "Enabling local LAN connectivity...".to_string();
            } else {
                self.snapshot.listener_port = None;
                self.snapshot.status = "Disabling local LAN connectivity...".to_string();
            }
        }

        if self
            .command_tx
            .send(NetworkCommand::SetAvailable(enabled))
            .is_err()
        {
            self.snapshot.last_error =
                "Could not send availability change to networking worker.".to_string();
        }
    }

    pub fn refresh_discovery(&self) {
        let _ = self.command_tx.send(NetworkCommand::RefreshDiscovery);
    }

    pub fn set_allow_unknown_devices(&mut self, enabled: bool) {
        if self.snapshot.allow_unknown_devices != enabled {
            self.snapshot.allow_unknown_devices = enabled;
            self.snapshot.status = if enabled {
                "Allowing unknown LAN devices.".to_string()
            } else {
                "Unknown devices now require approval.".to_string()
            };
        }
        if self
            .command_tx
            .send(NetworkCommand::SetAllowUnknownDevices(enabled))
            .is_err()
        {
            self.snapshot.last_error =
                "Could not send allow-unknown setting to networking worker.".to_string();
        }
    }

    pub fn replace_blocked_peers(&mut self, peers: &[BlockedPeer]) {
        self.snapshot.blocked_peers = peers.to_vec();
        if self
            .command_tx
            .send(NetworkCommand::ReplaceBlockedPeers(peers.to_vec()))
            .is_err()
        {
            self.snapshot.last_error =
                "Could not send blocked-device list to networking worker.".to_string();
        }
    }

    pub fn replace_trusted_peers(&mut self, peers: &[TrustedPeer]) {
        self.snapshot.trusted_peers = peers.to_vec();
        if self
            .command_tx
            .send(NetworkCommand::ReplaceTrustedPeers(peers.to_vec()))
            .is_err()
        {
            self.snapshot.last_error =
                "Could not send trusted-device list to networking worker.".to_string();
        }
    }

    pub fn connect_peer(&self, device_id: &str) {
        let _ = self
            .command_tx
            .send(NetworkCommand::ConnectPeer(device_id.to_string()));
    }

    pub fn set_presence(&self, presence: LocalPresence) {
        let _ = self.command_tx.send(NetworkCommand::SetPresence(presence));
    }

    pub fn set_device_name(&mut self, name: &str) {
        let trimmed = name.trim();
        let next = if trimmed.is_empty() {
            make_device_name()
        } else {
            trimmed.to_string()
        };

        if self.snapshot.device_name != next {
            self.snapshot.device_name = next.clone();
            self.snapshot.status = "Updating device name...".to_string();
        }

        if self
            .command_tx
            .send(NetworkCommand::SetDeviceName(next))
            .is_err()
        {
            self.snapshot.last_error =
                "Could not send device-name change to networking worker.".to_string();
        }
    }

    pub fn send_handoff(&self, connection_id: &str, title: &str, body: &str) {
        let _ = self.command_tx.send(NetworkCommand::SendHandoff {
            connection_id: connection_id.to_string(),
            title: title.to_string(),
            body: body.to_string(),
        });
    }

    pub fn send_artifact(
        &self,
        connection_id: &str,
        kind: &str,
        label: &str,
        module_id: Option<&str>,
        summary: &str,
        file_name: &str,
        text: &str,
    ) {
        let content_type = infer_text_content_type(kind, file_name);
        let _ = self.command_tx.send(NetworkCommand::SendArtifact {
            connection_id: connection_id.to_string(),
            kind: kind.to_string(),
            label: label.to_string(),
            module_id: module_id.unwrap_or_default().to_string(),
            summary: summary.to_string(),
            file_name: file_name.to_string(),
            content_type,
            transfer_encoding: ARTIFACT_ENCODING_UTF8.to_string(),
            byte_len: text.as_bytes().len() as u64,
            payload: text.to_string(),
        });
    }

    #[allow(dead_code)]
    pub fn send_artifact_bytes(
        &self,
        connection_id: &str,
        kind: &str,
        label: &str,
        module_id: Option<&str>,
        summary: &str,
        file_name: &str,
        content_type: &str,
        bytes: &[u8],
    ) {
        let resolved_content_type = if content_type.trim().is_empty() {
            CONTENT_TYPE_BINARY.to_string()
        } else {
            content_type.trim().to_string()
        };
        let _ = self.command_tx.send(NetworkCommand::SendArtifact {
            connection_id: connection_id.to_string(),
            kind: kind.to_string(),
            label: label.to_string(),
            module_id: module_id.unwrap_or_default().to_string(),
            summary: summary.to_string(),
            file_name: file_name.to_string(),
            content_type: resolved_content_type,
            transfer_encoding: ARTIFACT_ENCODING_BASE64.to_string(),
            byte_len: bytes.len() as u64,
            payload: BASE64.encode(bytes),
        });
    }

    pub fn send_session_event(
        &self,
        connection_id: &str,
        scope_module_id: &str,
        session_id: &str,
        event_type: &str,
        label: &str,
        content_type: &str,
        payload_text: &str,
    ) {
        let _ = self.command_tx.send(NetworkCommand::SendSessionEvent {
            connection_id: connection_id.to_string(),
            scope_module_id: scope_module_id.to_string(),
            session_id: session_id.to_string(),
            event_type: event_type.to_string(),
            label: label.to_string(),
            content_type: if content_type.trim().is_empty() {
                CONTENT_TYPE_JSON.to_string()
            } else {
                content_type.trim().to_string()
            },
            payload_text: payload_text.to_string(),
        });
    }

    pub fn disconnect_connection(&self, connection_id: &str) {
        let _ = self.command_tx.send(NetworkCommand::DisconnectConnection(
            connection_id.to_string(),
        ));
    }

    pub fn allow_pending_peer(&self, device_id: &str) {
        let _ = self
            .command_tx
            .send(NetworkCommand::AllowPendingPeer(device_id.to_string()));
    }

    pub fn trust_peer(&self, device_id: &str, device_name: &str) {
        let _ = self.command_tx.send(NetworkCommand::TrustPeer {
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
        });
    }

    pub fn deny_pending_peer(&self, device_id: &str) {
        let _ = self
            .command_tx
            .send(NetworkCommand::DenyPendingPeer(device_id.to_string()));
    }

    pub fn block_peer(&self, device_id: &str, device_name: &str) {
        let _ = self.command_tx.send(NetworkCommand::BlockPeer {
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
        });
    }

    pub fn unblock_peer(&self, device_id: &str) {
        let _ = self
            .command_tx
            .send(NetworkCommand::UnblockPeer(device_id.to_string()));
    }

    pub fn untrust_peer(&self, device_id: &str) {
        let _ = self
            .command_tx
            .send(NetworkCommand::UntrustPeer(device_id.to_string()));
    }

    pub fn clear_received_handoffs(&self) {
        let _ = self.command_tx.send(NetworkCommand::ClearReceivedHandoffs);
    }

    pub fn clear_received_artifacts(&self) {
        let _ = self.command_tx.send(NetworkCommand::ClearReceivedArtifacts);
    }

    pub fn clear_received_session_events(&self) {
        let _ = self
            .command_tx
            .send(NetworkCommand::ClearReceivedSessionEvents);
    }
}

impl Drop for NetworkController {
    fn drop(&mut self) {
        let _ = self.command_tx.send(NetworkCommand::Shutdown);
    }
}

#[derive(Debug)]
enum NetworkCommand {
    SetAvailable(bool),
    SetAllowUnknownDevices(bool),
    ReplaceBlockedPeers(Vec<BlockedPeer>),
    ReplaceTrustedPeers(Vec<TrustedPeer>),
    RefreshDiscovery,
    ConnectPeer(String),
    SetPresence(LocalPresence),
    SetDeviceName(String),
    SendHandoff {
        connection_id: String,
        title: String,
        body: String,
    },
    SendArtifact {
        connection_id: String,
        kind: String,
        label: String,
        module_id: String,
        summary: String,
        file_name: String,
        content_type: String,
        transfer_encoding: String,
        byte_len: u64,
        payload: String,
    },
    SendSessionEvent {
        connection_id: String,
        scope_module_id: String,
        session_id: String,
        event_type: String,
        label: String,
        content_type: String,
        payload_text: String,
    },
    DisconnectConnection(String),
    AllowPendingPeer(String),
    TrustPeer {
        device_id: String,
        device_name: String,
    },
    DenyPendingPeer(String),
    BlockPeer {
        device_id: String,
        device_name: String,
    },
    UnblockPeer(String),
    UntrustPeer(String),
    ClearReceivedHandoffs,
    ClearReceivedArtifacts,
    ClearReceivedSessionEvents,
    Shutdown,
}

#[derive(Debug)]
enum NetworkEvent {
    Snapshot(NetworkSnapshot),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NetworkPacket {
    #[serde(default = "default_protocol_family")]
    protocol_family: String,
    #[serde(default = "default_protocol_id")]
    protocol: String,
    #[serde(default = "default_protocol_version")]
    version: u8,
    #[serde(default = "default_min_supported_protocol_version")]
    min_supported_version: u8,
    #[serde(default = "default_max_supported_protocol_version")]
    max_supported_version: u8,
    kind: String,
    device_id: String,
    device_name: String,
    tcp_port: Option<u16>,
    #[serde(default)]
    status: Option<StatusPayload>,
    #[serde(default)]
    handoff: Option<HandoffPayload>,
    #[serde(default)]
    artifact: Option<ArtifactPayload>,
    #[serde(default)]
    session_event: Option<SessionEventPayload>,
    #[serde(default)]
    artifact_ack: Option<ArtifactAckPayload>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StatusPayload {
    active_tab: String,
    runtime_status: String,
    model_label: String,
    is_generating: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct HandoffPayload {
    handoff_id: String,
    title: String,
    body: String,
    sent_at_unix_ms: u128,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SessionEventPayload {
    event_id: String,
    #[serde(default)]
    scope_module_id: String,
    #[serde(default)]
    session_id: String,
    event_type: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    payload_text: String,
    sent_at_unix_ms: u128,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ArtifactPayload {
    artifact_id: String,
    kind: String,
    label: String,
    #[serde(default)]
    module_id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    content_type: String,
    #[serde(default = "default_artifact_transfer_encoding")]
    transfer_encoding: String,
    #[serde(default = "default_artifact_byte_len")]
    byte_len: u64,
    #[serde(default = "default_artifact_chunk_index")]
    chunk_index: u32,
    #[serde(default = "default_artifact_chunk_count")]
    chunk_count: u32,
    #[serde(default)]
    text: String,
    sent_at_unix_ms: u128,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ArtifactAckPayload {
    artifact_id: String,
    accepted: bool,
    duplicate: bool,
    #[serde(default)]
    message: String,
    received_at_unix_ms: u128,
}

impl NetworkPacket {
    fn discover(snapshot: &NetworkSnapshot) -> Self {
        Self {
            protocol_family: APP_PROTOCOL_FAMILY.to_string(),
            protocol: APP_PROTOCOL.to_string(),
            version: NETWORK_PROTOCOL_VERSION,
            min_supported_version: MIN_SUPPORTED_PROTOCOL_VERSION,
            max_supported_version: MAX_SUPPORTED_PROTOCOL_VERSION,
            kind: "discover".to_string(),
            device_id: snapshot.device_id.clone(),
            device_name: snapshot.device_name.clone(),
            tcp_port: None,
            status: None,
            handoff: None,
            artifact: None,
            session_event: None,
            artifact_ack: None,
        }
    }

    fn announce(snapshot: &NetworkSnapshot) -> Self {
        Self {
            protocol_family: APP_PROTOCOL_FAMILY.to_string(),
            protocol: APP_PROTOCOL.to_string(),
            version: NETWORK_PROTOCOL_VERSION,
            min_supported_version: MIN_SUPPORTED_PROTOCOL_VERSION,
            max_supported_version: MAX_SUPPORTED_PROTOCOL_VERSION,
            kind: "announce".to_string(),
            device_id: snapshot.device_id.clone(),
            device_name: snapshot.device_name.clone(),
            tcp_port: snapshot.listener_port,
            status: None,
            handoff: None,
            artifact: None,
            session_event: None,
            artifact_ack: None,
        }
    }

    fn hello(snapshot: &NetworkSnapshot) -> Self {
        Self {
            protocol_family: APP_PROTOCOL_FAMILY.to_string(),
            protocol: APP_PROTOCOL.to_string(),
            version: NETWORK_PROTOCOL_VERSION,
            min_supported_version: MIN_SUPPORTED_PROTOCOL_VERSION,
            max_supported_version: MAX_SUPPORTED_PROTOCOL_VERSION,
            kind: "hello".to_string(),
            device_id: snapshot.device_id.clone(),
            device_name: snapshot.device_name.clone(),
            tcp_port: snapshot.listener_port,
            status: Some(StatusPayload::from_presence(&snapshot.local_presence)),
            handoff: None,
            artifact: None,
            session_event: None,
            artifact_ack: None,
        }
    }

    fn status(snapshot: &NetworkSnapshot) -> Self {
        Self {
            protocol_family: APP_PROTOCOL_FAMILY.to_string(),
            protocol: APP_PROTOCOL.to_string(),
            version: NETWORK_PROTOCOL_VERSION,
            min_supported_version: MIN_SUPPORTED_PROTOCOL_VERSION,
            max_supported_version: MAX_SUPPORTED_PROTOCOL_VERSION,
            kind: "status".to_string(),
            device_id: snapshot.device_id.clone(),
            device_name: snapshot.device_name.clone(),
            tcp_port: snapshot.listener_port,
            status: Some(StatusPayload::from_presence(&snapshot.local_presence)),
            handoff: None,
            artifact: None,
            session_event: None,
            artifact_ack: None,
        }
    }

    fn handoff(snapshot: &NetworkSnapshot, title: &str, body: &str) -> Self {
        Self {
            protocol_family: APP_PROTOCOL_FAMILY.to_string(),
            protocol: APP_PROTOCOL.to_string(),
            version: NETWORK_PROTOCOL_VERSION,
            min_supported_version: MIN_SUPPORTED_PROTOCOL_VERSION,
            max_supported_version: MAX_SUPPORTED_PROTOCOL_VERSION,
            kind: "handoff".to_string(),
            device_id: snapshot.device_id.clone(),
            device_name: snapshot.device_name.clone(),
            tcp_port: snapshot.listener_port,
            status: Some(StatusPayload::from_presence(&snapshot.local_presence)),
            handoff: Some(HandoffPayload {
                handoff_id: format!("handoff-{}-{}", snapshot.device_id, now_unix_ms()),
                title: title.trim().to_string(),
                body: body.trim().to_string(),
                sent_at_unix_ms: now_unix_ms(),
            }),
            artifact: None,
            session_event: None,
            artifact_ack: None,
        }
    }

    fn artifact(snapshot: &NetworkSnapshot, artifact: ArtifactPayload) -> Self {
        Self {
            protocol_family: APP_PROTOCOL_FAMILY.to_string(),
            protocol: APP_PROTOCOL.to_string(),
            version: NETWORK_PROTOCOL_VERSION,
            min_supported_version: MIN_SUPPORTED_PROTOCOL_VERSION,
            max_supported_version: MAX_SUPPORTED_PROTOCOL_VERSION,
            kind: "artifact".to_string(),
            device_id: snapshot.device_id.clone(),
            device_name: snapshot.device_name.clone(),
            tcp_port: snapshot.listener_port,
            status: Some(StatusPayload::from_presence(&snapshot.local_presence)),
            handoff: None,
            artifact: Some(artifact),
            session_event: None,
            artifact_ack: None,
        }
    }

    fn session_event(snapshot: &NetworkSnapshot, event: SessionEventPayload) -> Self {
        Self {
            protocol_family: APP_PROTOCOL_FAMILY.to_string(),
            protocol: APP_PROTOCOL.to_string(),
            version: NETWORK_PROTOCOL_VERSION,
            min_supported_version: MIN_SUPPORTED_PROTOCOL_VERSION,
            max_supported_version: MAX_SUPPORTED_PROTOCOL_VERSION,
            kind: "session_event".to_string(),
            device_id: snapshot.device_id.clone(),
            device_name: snapshot.device_name.clone(),
            tcp_port: snapshot.listener_port,
            status: Some(StatusPayload::from_presence(&snapshot.local_presence)),
            handoff: None,
            artifact: None,
            session_event: Some(event),
            artifact_ack: None,
        }
    }

    fn artifact_ack(
        snapshot: &NetworkSnapshot,
        artifact_id: &str,
        accepted: bool,
        duplicate: bool,
        message: &str,
    ) -> Self {
        Self {
            protocol_family: APP_PROTOCOL_FAMILY.to_string(),
            protocol: APP_PROTOCOL.to_string(),
            version: NETWORK_PROTOCOL_VERSION,
            min_supported_version: MIN_SUPPORTED_PROTOCOL_VERSION,
            max_supported_version: MAX_SUPPORTED_PROTOCOL_VERSION,
            kind: "artifact_ack".to_string(),
            device_id: snapshot.device_id.clone(),
            device_name: snapshot.device_name.clone(),
            tcp_port: snapshot.listener_port,
            status: Some(StatusPayload::from_presence(&snapshot.local_presence)),
            handoff: None,
            artifact: None,
            session_event: None,
            artifact_ack: Some(ArtifactAckPayload {
                artifact_id: artifact_id.trim().to_string(),
                accepted,
                duplicate,
                message: message.trim().to_string(),
                received_at_unix_ms: now_unix_ms(),
            }),
        }
    }

    fn protocol_matches(&self) -> bool {
        self.protocol == APP_PROTOCOL || self.protocol_family == APP_PROTOCOL_FAMILY
    }

    fn protocol_compatible(&self) -> bool {
        if self.min_supported_version > self.max_supported_version {
            return false;
        }
        (MIN_SUPPORTED_PROTOCOL_VERSION..=MAX_SUPPORTED_PROTOCOL_VERSION).contains(&self.version)
            && self.min_supported_version <= MAX_SUPPORTED_PROTOCOL_VERSION
            && self.max_supported_version >= MIN_SUPPORTED_PROTOCOL_VERSION
    }

    fn compatibility_error(&self) -> Option<String> {
        if !self.protocol_matches() {
            return Some(format!(
                "Ignored LAN peer on incompatible protocol family (`{}` / `{}`).",
                self.protocol_family, self.protocol
            ));
        }
        if self.min_supported_version > self.max_supported_version {
            return Some(format!(
                "Ignored LAN peer with invalid protocol range {}-{}.",
                self.min_supported_version, self.max_supported_version
            ));
        }
        if !self.protocol_compatible() {
            return Some(format!(
                "Ignored LAN peer using protocol v{} (it supports {}-{}, this build supports {}-{}).",
                self.version,
                self.min_supported_version,
                self.max_supported_version,
                MIN_SUPPORTED_PROTOCOL_VERSION,
                MAX_SUPPORTED_PROTOCOL_VERSION
            ));
        }
        None
    }
}

impl ArtifactPayload {
    fn fragment(&self) -> &str {
        self.text.as_str()
    }

    fn normalized_transfer_encoding(&self) -> &str {
        if self.transfer_encoding.trim().is_empty() {
            ARTIFACT_ENCODING_UTF8
        } else {
            self.transfer_encoding.trim()
        }
    }

    fn normalized_chunk_count(&self) -> u32 {
        self.chunk_count.max(1)
    }

    fn normalized_content_type(&self) -> String {
        if self.content_type.trim().is_empty() {
            infer_text_content_type(&self.kind, &self.file_name)
        } else {
            self.content_type.trim().to_string()
        }
    }

    fn declared_byte_len(&self) -> u64 {
        if self.byte_len > 0 {
            self.byte_len
        } else if self.normalized_transfer_encoding() == ARTIFACT_ENCODING_UTF8 {
            self.fragment().as_bytes().len() as u64
        } else {
            0
        }
    }

    fn expected_encoded_len(&self) -> Result<usize, String> {
        expected_encoded_len(
            self.declared_byte_len(),
            self.normalized_transfer_encoding(),
        )
    }

    fn validate_chunk_metadata(&self) -> Result<(), String> {
        let chunk_count = self.normalized_chunk_count();
        if chunk_count == 0 {
            return Err("Chunk count must be at least 1.".to_string());
        }
        if self.chunk_index >= chunk_count {
            return Err(format!(
                "Chunk index {} is outside the declared chunk range {}.",
                self.chunk_index, chunk_count
            ));
        }
        let declared_byte_len = self.declared_byte_len();
        if declared_byte_len as usize > MAX_ARTIFACT_TOTAL_BYTES {
            return Err(format!(
                "Transfer too large ({} bytes; limit {} bytes).",
                declared_byte_len, MAX_ARTIFACT_TOTAL_BYTES
            ));
        }
        let fragment_len = self.fragment().as_bytes().len();
        if fragment_len > MAX_ARTIFACT_CHUNK_BYTES {
            return Err(format!(
                "Chunk too large ({} bytes; limit {} bytes).",
                fragment_len, MAX_ARTIFACT_CHUNK_BYTES
            ));
        }
        match self.normalized_transfer_encoding() {
            ARTIFACT_ENCODING_UTF8 | ARTIFACT_ENCODING_BASE64 => {}
            other => {
                return Err(format!("Unsupported transfer encoding `{other}`."));
            }
        }
        let expected_encoded = self.expected_encoded_len()?;
        if expected_encoded > max_encoded_payload_storage(declared_byte_len) {
            return Err("Encoded transfer exceeds local LAN limits.".to_string());
        }
        Ok(())
    }
}

impl SessionEventPayload {
    fn normalize(&mut self) {
        self.event_id = self.event_id.trim().to_string();
        self.scope_module_id = self.scope_module_id.trim().to_string();
        self.session_id = self.session_id.trim().to_string();
        self.event_type = self.event_type.trim().to_string();
        self.label = self.label.trim().to_string();
        self.content_type = if self.content_type.trim().is_empty() {
            CONTENT_TYPE_JSON.to_string()
        } else {
            self.content_type.trim().to_string()
        };
        if self.sent_at_unix_ms == 0 {
            self.sent_at_unix_ms = now_unix_ms();
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.event_type.trim().is_empty() {
            return Err("Session event type cannot be empty.".to_string());
        }
        if self.payload_text.as_bytes().len() > MAX_SESSION_EVENT_TEXT_BYTES {
            return Err(format!(
                "Session event payload is too large ({} bytes; limit {} bytes).",
                self.payload_text.as_bytes().len(),
                MAX_SESSION_EVENT_TEXT_BYTES
            ));
        }
        Ok(())
    }
}

fn default_protocol_id() -> String {
    APP_PROTOCOL.to_string()
}

fn default_protocol_family() -> String {
    APP_PROTOCOL_FAMILY.to_string()
}

fn default_protocol_version() -> u8 {
    NETWORK_PROTOCOL_VERSION
}

fn default_min_supported_protocol_version() -> u8 {
    MIN_SUPPORTED_PROTOCOL_VERSION
}

fn default_max_supported_protocol_version() -> u8 {
    MAX_SUPPORTED_PROTOCOL_VERSION
}

fn default_artifact_transfer_encoding() -> String {
    ARTIFACT_ENCODING_UTF8.to_string()
}

fn default_artifact_byte_len() -> u64 {
    0
}

fn default_artifact_chunk_index() -> u32 {
    0
}

fn default_artifact_chunk_count() -> u32 {
    1
}

impl StatusPayload {
    fn from_presence(presence: &LocalPresence) -> Self {
        Self {
            active_tab: presence.active_tab.clone(),
            runtime_status: presence.runtime_status.clone(),
            model_label: presence.model_label.clone(),
            is_generating: presence.is_generating,
        }
    }

    fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.active_tab.trim().is_empty() {
            parts.push(format!("Tab: {}", self.active_tab.trim()));
        }
        if !self.runtime_status.trim().is_empty() {
            parts.push(self.runtime_status.trim().to_string());
        }
        if !self.model_label.trim().is_empty() {
            parts.push(format!("Model: {}", self.model_label.trim()));
        }
        if self.is_generating {
            parts.push("Generating".to_string());
        }
        if parts.is_empty() {
            "No shared status yet.".to_string()
        } else {
            parts.join(" | ")
        }
    }
}

#[derive(Debug, Clone)]
struct KnownPeer {
    device_id: String,
    device_name: String,
    address: SocketAddr,
    host_port: u16,
    last_seen: Instant,
}

#[derive(Debug)]
struct LiveConnection {
    connection_id: String,
    stream: TcpStream,
    address: SocketAddr,
    peer_id: Option<String>,
    peer_name: Option<String>,
    inbound: bool,
    connected_at: Instant,
    read_buffer: Vec<u8>,
    last_status: Option<StatusPayload>,
    last_status_at: Option<Instant>,
}

#[derive(Debug, Clone)]
struct ReceivedHandoffState {
    handoff_id: String,
    from_device_id: String,
    from_device_name: String,
    from_address: String,
    title: String,
    body: String,
    received_at: Instant,
}

#[derive(Debug, Clone)]
struct ReceivedArtifactState {
    artifact_id: String,
    from_device_id: String,
    from_device_name: String,
    from_address: String,
    kind: String,
    label: String,
    module_id: String,
    summary: String,
    file_name: String,
    content_type: String,
    transfer_encoding: String,
    byte_len: u64,
    chunk_count: u32,
    text: String,
    data_base64: String,
    received_at: Instant,
}

#[derive(Debug, Clone)]
struct ReceivedSessionEventState {
    event_id: String,
    from_device_id: String,
    from_device_name: String,
    from_address: String,
    scope_module_id: String,
    session_id: String,
    event_type: String,
    label: String,
    content_type: String,
    payload_text: String,
    received_at_unix_ms: u64,
    received_at: Instant,
}

#[derive(Debug, Clone)]
struct OutgoingArtifactDeliveryState {
    artifact_id: String,
    connection_id: String,
    to_device_id: String,
    to_device_name: String,
    to_address: String,
    kind: String,
    label: String,
    module_id: String,
    summary: String,
    file_name: String,
    content_type: String,
    transfer_encoding: String,
    byte_len: u64,
    chunk_count: u32,
    status: String,
    attempts: u32,
    waiting_for_ack: bool,
    packets: Vec<NetworkPacket>,
    last_attempt_at: Instant,
    updated_at: Instant,
}

#[derive(Debug, Clone)]
struct IncomingArtifactAssemblyState {
    artifact_id: String,
    from_device_id: String,
    from_device_name: String,
    from_address: String,
    kind: String,
    label: String,
    module_id: String,
    summary: String,
    file_name: String,
    content_type: String,
    transfer_encoding: String,
    byte_len: u64,
    chunk_count: u32,
    chunks: Vec<Option<String>>,
    encoded_bytes_received: usize,
    received_chunks: u32,
    updated_at: Instant,
}

#[derive(Debug, Clone)]
struct PreparedArtifactTransfer {
    artifact: ArtifactPayload,
    packets: Vec<NetworkPacket>,
}

#[derive(Debug, Clone)]
struct BlockedPeerState {
    device_id: String,
    device_name: String,
    address: String,
    last_seen: Option<Instant>,
}

#[derive(Debug, Clone)]
struct TrustedPeerState {
    device_id: String,
    device_name: String,
    address: String,
    last_seen: Option<Instant>,
}

#[derive(Debug, Clone)]
struct PendingPeerRequestState {
    device_id: String,
    device_name: String,
    address: String,
    requested_at: Instant,
}

struct NetworkService {
    snapshot: NetworkSnapshot,
    command_rx: Receiver<NetworkCommand>,
    event_tx: Sender<NetworkEvent>,
    client_socket: Option<UdpSocket>,
    discovery_listener: Option<UdpSocket>,
    tcp_listener: Option<TcpListener>,
    peers: HashMap<String, KnownPeer>,
    connections: HashMap<String, LiveConnection>,
    approved_peers: HashSet<String>,
    trusted_peers: HashMap<String, TrustedPeerState>,
    blocked_peers: HashMap<String, BlockedPeerState>,
    pending_requests: HashMap<String, PendingPeerRequestState>,
    received_handoffs: Vec<ReceivedHandoffState>,
    received_artifacts: Vec<ReceivedArtifactState>,
    received_session_events: Vec<ReceivedSessionEventState>,
    outgoing_artifacts: Vec<OutgoingArtifactDeliveryState>,
    incoming_artifact_assemblies: HashMap<String, IncomingArtifactAssemblyState>,
    seen_artifact_ids: HashMap<String, Instant>,
    seen_session_event_ids: HashMap<String, Instant>,
    next_connection_id: u64,
    last_discovery_sent: Instant,
    last_status_sent: Instant,
    last_snapshot_sent: Instant,
    dirty: bool,
}

impl NetworkService {
    fn new(
        snapshot: NetworkSnapshot,
        command_rx: Receiver<NetworkCommand>,
        event_tx: Sender<NetworkEvent>,
    ) -> Self {
        let client_socket = Self::bind_client_socket();
        let mut service = Self {
            snapshot,
            command_rx,
            event_tx,
            client_socket,
            discovery_listener: None,
            tcp_listener: None,
            peers: HashMap::new(),
            connections: HashMap::new(),
            approved_peers: HashSet::new(),
            trusted_peers: HashMap::new(),
            blocked_peers: HashMap::new(),
            pending_requests: HashMap::new(),
            received_handoffs: Vec::new(),
            received_artifacts: Vec::new(),
            received_session_events: Vec::new(),
            outgoing_artifacts: Vec::new(),
            incoming_artifact_assemblies: HashMap::new(),
            seen_artifact_ids: HashMap::new(),
            seen_session_event_ids: HashMap::new(),
            next_connection_id: 1,
            last_discovery_sent: Instant::now() - DISCOVERY_INTERVAL,
            last_status_sent: Instant::now() - STATUS_INTERVAL,
            last_snapshot_sent: Instant::now() - SNAPSHOT_INTERVAL,
            dirty: true,
        };
        if service.client_socket.is_none() {
            service.snapshot.last_error =
                "Discovery socket failed to start. Local networking may be unavailable."
                    .to_string();
        }
        service
    }

    fn run(&mut self) {
        self.push_snapshot();
        loop {
            while let Ok(command) = self.command_rx.try_recv() {
                if self.handle_command(command) {
                    self.shutdown_all();
                    self.push_snapshot();
                    return;
                }
            }

            self.poll_discovery_responses();
            self.poll_discovery_requests();
            self.poll_tcp_accepts();
            self.poll_connections();
            self.poll_pending_artifact_acks();
            self.prune_stale_peers();
            self.prune_recent_artifact_ids();
            self.prune_recent_session_event_ids();
            self.prune_incoming_artifact_assemblies();
            self.prune_outgoing_artifacts();

            if self.last_discovery_sent.elapsed() >= DISCOVERY_INTERVAL {
                self.send_discover();
            }
            if self.last_status_sent.elapsed() >= STATUS_INTERVAL {
                self.broadcast_status();
            }

            if self.dirty || self.last_snapshot_sent.elapsed() >= SNAPSHOT_INTERVAL {
                self.push_snapshot();
            }

            std::thread::sleep(LOOP_SLEEP);
        }
    }

    fn record_protocol_notice(&mut self, message: String) {
        if self.snapshot.protocol_notice != message {
            self.snapshot.protocol_notice = message;
            self.dirty = true;
        }
    }

    fn bind_client_socket() -> Option<UdpSocket> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
        let _ = socket.set_broadcast(true);
        let _ = socket.set_nonblocking(true);
        Some(socket)
    }

    fn handle_command(&mut self, command: NetworkCommand) -> bool {
        match command {
            NetworkCommand::SetAvailable(enabled) => {
                self.set_available(enabled);
                false
            }
            NetworkCommand::SetAllowUnknownDevices(enabled) => {
                self.set_allow_unknown_devices(enabled);
                false
            }
            NetworkCommand::ReplaceBlockedPeers(peers) => {
                self.replace_blocked_peers(peers);
                false
            }
            NetworkCommand::ReplaceTrustedPeers(peers) => {
                self.replace_trusted_peers(peers);
                false
            }
            NetworkCommand::RefreshDiscovery => {
                self.send_discover();
                false
            }
            NetworkCommand::ConnectPeer(device_id) => {
                self.connect_peer(&device_id);
                false
            }
            NetworkCommand::SetPresence(presence) => {
                self.set_presence(presence);
                false
            }
            NetworkCommand::SetDeviceName(name) => {
                self.set_device_name(name);
                false
            }
            NetworkCommand::SendHandoff {
                connection_id,
                title,
                body,
            } => {
                self.send_handoff(&connection_id, &title, &body);
                false
            }
            NetworkCommand::SendArtifact {
                connection_id,
                kind,
                label,
                module_id,
                summary,
                file_name,
                content_type,
                transfer_encoding,
                byte_len,
                payload,
            } => {
                self.send_artifact(
                    &connection_id,
                    &kind,
                    &label,
                    &module_id,
                    &summary,
                    &file_name,
                    &content_type,
                    &transfer_encoding,
                    byte_len,
                    &payload,
                );
                false
            }
            NetworkCommand::SendSessionEvent {
                connection_id,
                scope_module_id,
                session_id,
                event_type,
                label,
                content_type,
                payload_text,
            } => {
                self.send_session_event(
                    &connection_id,
                    &scope_module_id,
                    &session_id,
                    &event_type,
                    &label,
                    &content_type,
                    &payload_text,
                );
                false
            }
            NetworkCommand::DisconnectConnection(connection_id) => {
                self.disconnect_connection(&connection_id);
                false
            }
            NetworkCommand::AllowPendingPeer(device_id) => {
                self.allow_pending_peer(&device_id);
                false
            }
            NetworkCommand::TrustPeer {
                device_id,
                device_name,
            } => {
                self.trust_peer(&device_id, &device_name);
                false
            }
            NetworkCommand::DenyPendingPeer(device_id) => {
                self.deny_pending_peer(&device_id);
                false
            }
            NetworkCommand::BlockPeer {
                device_id,
                device_name,
            } => {
                self.block_peer(&device_id, &device_name);
                false
            }
            NetworkCommand::UnblockPeer(device_id) => {
                self.unblock_peer(&device_id);
                false
            }
            NetworkCommand::UntrustPeer(device_id) => {
                self.untrust_peer(&device_id);
                false
            }
            NetworkCommand::ClearReceivedHandoffs => {
                self.received_handoffs.clear();
                self.dirty = true;
                false
            }
            NetworkCommand::ClearReceivedArtifacts => {
                self.received_artifacts.clear();
                self.dirty = true;
                false
            }
            NetworkCommand::ClearReceivedSessionEvents => {
                self.received_session_events.clear();
                self.dirty = true;
                false
            }
            NetworkCommand::Shutdown => true,
        }
    }

    fn set_presence(&mut self, presence: LocalPresence) {
        if self.snapshot.local_presence != presence {
            self.snapshot.local_presence = presence;
            self.broadcast_status();
            self.dirty = true;
        }
    }

    fn set_allow_unknown_devices(&mut self, enabled: bool) {
        if self.snapshot.allow_unknown_devices != enabled {
            self.snapshot.allow_unknown_devices = enabled;
            self.snapshot.status = if enabled {
                "Unknown LAN devices may connect automatically.".to_string()
            } else {
                "Unknown LAN devices now require approval.".to_string()
            };
            self.dirty = true;
        }
    }

    fn replace_blocked_peers(&mut self, peers: Vec<BlockedPeer>) {
        self.blocked_peers = peers
            .into_iter()
            .map(|peer| {
                (
                    peer.device_id.clone(),
                    BlockedPeerState {
                        device_id: peer.device_id,
                        device_name: peer.device_name,
                        address: peer.address,
                        last_seen: None,
                    },
                )
            })
            .collect();
        self.trusted_peers
            .retain(|device_id, _| !self.blocked_peers.contains_key(device_id));
        self.approved_peers
            .retain(|device_id| !self.blocked_peers.contains_key(device_id));
        self.peers
            .retain(|device_id, _| !self.blocked_peers.contains_key(device_id));
        self.pending_requests
            .retain(|device_id, _| !self.blocked_peers.contains_key(device_id));
        let blocked_ids = self.blocked_peers.keys().cloned().collect::<Vec<_>>();
        for device_id in blocked_ids {
            self.disconnect_peer_id(&device_id);
        }
        self.snapshot.status = "Updated blocked devices.".to_string();
        self.dirty = true;
    }

    fn replace_trusted_peers(&mut self, peers: Vec<TrustedPeer>) {
        self.trusted_peers = peers
            .into_iter()
            .filter(|peer| !peer.device_id.trim().is_empty())
            .filter(|peer| !self.blocked_peers.contains_key(&peer.device_id))
            .map(|peer| {
                (
                    peer.device_id.clone(),
                    TrustedPeerState {
                        device_id: peer.device_id,
                        device_name: peer.device_name,
                        address: peer.address,
                        last_seen: None,
                    },
                )
            })
            .collect();
        self.approved_peers
            .extend(self.trusted_peers.keys().cloned());
        self.pending_requests
            .retain(|device_id, _| !self.trusted_peers.contains_key(device_id));
        self.snapshot.status = "Updated trusted devices.".to_string();
        self.dirty = true;
    }

    fn set_device_name(&mut self, name: String) {
        let trimmed = name.trim();
        let next = if trimmed.is_empty() {
            make_device_name()
        } else {
            trimmed.to_string()
        };

        if self.snapshot.device_name == next {
            return;
        }

        self.snapshot.device_name = next;
        self.snapshot.status = "Updated local device name.".to_string();
        self.send_discover();
        self.broadcast_status();
        self.dirty = true;
    }

    fn set_available(&mut self, enabled: bool) {
        if enabled == self.snapshot.available_for_connectivity {
            return;
        }

        if enabled {
            let discovery_listener = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT))
            {
                Ok(socket) => {
                    let _ = socket.set_nonblocking(true);
                    Some(socket)
                }
                Err(error) => {
                    self.snapshot.last_error =
                        format!("Could not bind discovery port {DISCOVERY_PORT}: {error}");
                    self.snapshot.status = "Networking available toggle failed.".to_string();
                    self.dirty = true;
                    return;
                }
            };

            let tcp_listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)) {
                Ok(listener) => {
                    let _ = listener.set_nonblocking(true);
                    Some(listener)
                }
                Err(error) => {
                    self.snapshot.last_error =
                        format!("Could not open host listener for LAN clients: {error}");
                    self.snapshot.status = "Networking host listener failed.".to_string();
                    self.dirty = true;
                    return;
                }
            };

            self.snapshot.available_for_connectivity = true;
            self.snapshot.listener_port = tcp_listener
                .as_ref()
                .and_then(|listener| listener.local_addr().ok())
                .map(|addr| addr.port());
            self.discovery_listener = discovery_listener;
            self.tcp_listener = tcp_listener;
            self.snapshot.last_error.clear();
            self.snapshot.status = format!(
                "Available for local LAN connectivity on TCP port {}.",
                self.snapshot.listener_port.unwrap_or_default()
            );
            self.send_discover();
        } else {
            self.snapshot.available_for_connectivity = false;
            self.snapshot.listener_port = None;
            self.discovery_listener = None;
            self.tcp_listener = None;
            self.shutdown_all_connections();
            self.snapshot.status = "Connectivity host disabled.".to_string();
        }

        self.dirty = true;
    }

    fn allow_pending_peer(&mut self, device_id: &str) {
        if let Some(request) = self.pending_requests.remove(device_id) {
            self.approved_peers.insert(device_id.to_string());
            self.snapshot.status = format!(
                "Allowed {}. Ask that device to reconnect.",
                request.device_name
            );
            self.snapshot.last_error.clear();
            self.dirty = true;
        }
    }

    fn trust_peer(&mut self, device_id: &str, device_name: &str) {
        if device_id.trim().is_empty() {
            return;
        }
        let display_name = if device_name.trim().is_empty() {
            device_id.to_string()
        } else {
            device_name.trim().to_string()
        };
        let address = self
            .peers
            .get(device_id)
            .map(|peer| peer.address.ip().to_string())
            .or_else(|| {
                self.connections
                    .values()
                    .find(|connection| connection.peer_id.as_deref() == Some(device_id))
                    .map(|connection| connection.address.to_string())
            })
            .or_else(|| {
                self.pending_requests
                    .get(device_id)
                    .map(|request| request.address.clone())
            })
            .unwrap_or_default();
        self.trusted_peers.insert(
            device_id.to_string(),
            TrustedPeerState {
                device_id: device_id.to_string(),
                device_name: display_name.clone(),
                address,
                last_seen: Some(Instant::now()),
            },
        );
        self.approved_peers.insert(device_id.to_string());
        self.pending_requests.remove(device_id);
        self.snapshot.status = format!(
            "Trusted {}. Future connections will be approved automatically.",
            display_name
        );
        self.snapshot.last_error.clear();
        self.dirty = true;
    }

    fn deny_pending_peer(&mut self, device_id: &str) {
        if let Some(request) = self.pending_requests.remove(device_id) {
            self.snapshot.status = format!("Denied {} for now.", request.device_name);
            self.dirty = true;
        }
    }

    fn block_peer(&mut self, device_id: &str, device_name: &str) {
        if device_id.trim().is_empty() {
            return;
        }
        let display_name = if device_name.trim().is_empty() {
            device_id.to_string()
        } else {
            device_name.trim().to_string()
        };
        let address = self
            .peers
            .get(device_id)
            .map(|peer| peer.address.to_string())
            .or_else(|| {
                self.pending_requests
                    .get(device_id)
                    .map(|req| req.address.clone())
            })
            .unwrap_or_default();
        self.blocked_peers.insert(
            device_id.to_string(),
            BlockedPeerState {
                device_id: device_id.to_string(),
                device_name: display_name.clone(),
                address,
                last_seen: Some(Instant::now()),
            },
        );
        self.trusted_peers.remove(device_id);
        self.approved_peers.remove(device_id);
        self.pending_requests.remove(device_id);
        self.peers.remove(device_id);
        self.disconnect_peer_id(device_id);
        self.snapshot.status = format!("{display_name} is now blocked.");
        self.dirty = true;
    }

    fn unblock_peer(&mut self, device_id: &str) {
        if let Some(peer) = self.blocked_peers.remove(device_id) {
            self.snapshot.status = format!("Unblocked {}.", peer.device_name);
            self.dirty = true;
        }
    }

    fn untrust_peer(&mut self, device_id: &str) {
        if let Some(peer) = self.trusted_peers.remove(device_id) {
            self.approved_peers.remove(device_id);
            self.snapshot.status = format!("Removed {} from trusted devices.", peer.device_name);
            self.dirty = true;
        }
    }

    fn note_trusted_peer_activity(&mut self, device_id: &str, device_name: &str, address: String) {
        if let Some(peer) = self.trusted_peers.get_mut(device_id) {
            if !device_name.trim().is_empty() {
                peer.device_name = device_name.trim().to_string();
            }
            if !address.trim().is_empty() {
                peer.address = address;
            }
            peer.last_seen = Some(Instant::now());
            self.dirty = true;
        }
    }

    fn send_discover(&mut self) {
        let Some(socket) = &self.client_socket else {
            self.snapshot.last_error =
                "Discovery socket is unavailable, so LAN scans cannot run.".to_string();
            self.dirty = true;
            return;
        };

        let packet = NetworkPacket::discover(&self.snapshot);
        if let Ok(bytes) = serde_json::to_vec(&packet) {
            let _ = socket.send_to(
                &bytes,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DISCOVERY_PORT),
            );
            self.last_discovery_sent = Instant::now();
            self.snapshot.status = if self.snapshot.available_for_connectivity {
                "Hosting on the LAN and scanning for nearby Chatty-EDU instances.".to_string()
            } else {
                "Scanning the local LAN for available Chatty-EDU instances.".to_string()
            };
            self.dirty = true;
        }
    }

    fn poll_discovery_responses(&mut self) {
        let Some(socket) = self
            .client_socket
            .as_ref()
            .and_then(|socket| socket.try_clone().ok())
        else {
            return;
        };

        let mut buffer = [0u8; 4096];
        loop {
            match socket.recv_from(&mut buffer) {
                Ok((len, src)) => self.handle_udp_packet(&buffer[..len], src),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    self.snapshot.last_error =
                        format!("Discovery receive error on client socket: {error}");
                    self.dirty = true;
                    break;
                }
            }
        }
    }

    fn poll_discovery_requests(&mut self) {
        let Some(socket) = self
            .discovery_listener
            .as_ref()
            .and_then(|socket| socket.try_clone().ok())
        else {
            return;
        };

        let mut buffer = [0u8; 4096];
        loop {
            match socket.recv_from(&mut buffer) {
                Ok((len, src)) => self.handle_udp_packet(&buffer[..len], src),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    self.snapshot.last_error =
                        format!("Discovery receive error on host socket: {error}");
                    self.dirty = true;
                    break;
                }
            }
        }
    }

    fn handle_udp_packet(&mut self, bytes: &[u8], src: SocketAddr) {
        let Ok(packet) = serde_json::from_slice::<NetworkPacket>(bytes) else {
            return;
        };
        if packet.device_id == self.snapshot.device_id {
            return;
        }
        if let Some(reason) = packet.compatibility_error() {
            self.record_protocol_notice(format!("{} From {}.", reason, src.ip()));
            return;
        }

        if self.blocked_peers.contains_key(&packet.device_id) {
            if let Some(blocked) = self.blocked_peers.get_mut(&packet.device_id) {
                blocked.device_name = packet.device_name.clone();
                blocked.address = src.ip().to_string();
                blocked.last_seen = Some(Instant::now());
            }
            self.peers.remove(&packet.device_id);
            self.dirty = true;
            return;
        }

        self.note_trusted_peer_activity(
            &packet.device_id,
            &packet.device_name,
            src.ip().to_string(),
        );

        match packet.kind.as_str() {
            "discover" => {
                if self.snapshot.available_for_connectivity {
                    self.send_announce(src);
                }
            }
            "announce" => {
                let Some(host_port) = packet.tcp_port else {
                    return;
                };
                self.peers.insert(
                    packet.device_id.clone(),
                    KnownPeer {
                        device_id: packet.device_id,
                        device_name: packet.device_name,
                        address: SocketAddr::new(src.ip(), host_port),
                        host_port,
                        last_seen: Instant::now(),
                    },
                );
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn send_announce(&mut self, target: SocketAddr) {
        let Some(socket) = &self.discovery_listener else {
            return;
        };
        let packet = NetworkPacket::announce(&self.snapshot);
        if let Ok(bytes) = serde_json::to_vec(&packet) {
            let _ = socket.send_to(&bytes, target);
        }
    }

    fn broadcast_status(&mut self) {
        if self.connections.is_empty() {
            self.last_status_sent = Instant::now();
            return;
        }
        let packet = NetworkPacket::status(&self.snapshot);
        let sent = self.send_packet_to_all(&packet);
        if sent > 0 {
            self.snapshot.status = format!("Shared status with {sent} connected device(s).");
        }
        self.last_status_sent = Instant::now();
        self.dirty = true;
    }

    fn connect_peer(&mut self, device_id: &str) {
        if self.blocked_peers.contains_key(device_id) {
            self.snapshot.last_error =
                format!("Peer `{device_id}` is blocked and cannot be connected.");
            self.snapshot.status = "Connection refused by local block list.".to_string();
            self.dirty = true;
            return;
        }
        let Some(peer) = self.peers.get(device_id).cloned() else {
            self.snapshot.last_error =
                format!("Peer `{device_id}` is no longer visible on the LAN.");
            self.dirty = true;
            return;
        };

        if self
            .connections
            .values()
            .any(|connection| connection.peer_id.as_deref() == Some(device_id))
        {
            self.snapshot.status = format!("Already connected to {}.", peer.device_name);
            self.dirty = true;
            return;
        }

        match TcpStream::connect_timeout(&peer.address, Duration::from_millis(900)) {
            Ok(mut stream) => {
                let _ = stream.set_nonblocking(true);
                let connection_id = self.next_connection_id();
                self.approved_peers.insert(peer.device_id.clone());
                self.pending_requests.remove(&peer.device_id);
                if let Err(error) = self.send_hello(&mut stream) {
                    self.snapshot.last_error = format!(
                        "Connected to {} but handshake failed: {error}",
                        peer.device_name
                    );
                }
                self.connections.insert(
                    connection_id.clone(),
                    LiveConnection {
                        connection_id: connection_id.clone(),
                        stream,
                        address: peer.address,
                        peer_id: Some(peer.device_id.clone()),
                        peer_name: Some(peer.device_name.clone()),
                        inbound: false,
                        connected_at: Instant::now(),
                        read_buffer: Vec::new(),
                        last_status: None,
                        last_status_at: None,
                    },
                );
                self.snapshot.last_error.clear();
                self.snapshot.status = format!("Connected to {}.", peer.device_name);
                self.broadcast_status();
                self.dirty = true;
            }
            Err(error) => {
                self.snapshot.last_error = format!(
                    "Could not connect to {} at {}: {error}",
                    peer.device_name, peer.address
                );
                self.snapshot.status = "Connection attempt failed.".to_string();
                self.dirty = true;
            }
        }
    }

    fn disconnect_connection(&mut self, connection_id: &str) {
        if let Some(connection) = self.connections.remove(connection_id) {
            self.fail_pending_artifacts_for_connection(
                connection_id,
                "Connection closed before delivery confirmation.",
            );
            let _ = connection.stream.shutdown(Shutdown::Both);
            self.snapshot.status = format!(
                "Disconnected from {}.",
                connection
                    .peer_name
                    .clone()
                    .unwrap_or_else(|| connection.address.to_string())
            );
            self.dirty = true;
        }
    }

    fn send_handoff(&mut self, connection_id: &str, title: &str, body: &str) {
        let Some(connection) = self.connections.get_mut(connection_id) else {
            self.snapshot.last_error =
                "That connection is no longer active, so the handoff could not be sent."
                    .to_string();
            self.dirty = true;
            return;
        };

        let packet = NetworkPacket::handoff(&self.snapshot, title, body);
        match write_packet(&mut connection.stream, &packet) {
            Ok(()) => {
                self.snapshot.last_error.clear();
                self.snapshot.status = format!(
                    "Sent handoff to {}.",
                    connection
                        .peer_name
                        .clone()
                        .unwrap_or_else(|| connection.address.to_string())
                );
            }
            Err(error) => {
                self.snapshot.last_error = format!("Could not send handoff: {error}");
                self.snapshot.status = "Handoff send failed.".to_string();
            }
        }
        self.dirty = true;
    }

    fn send_artifact(
        &mut self,
        connection_id: &str,
        kind: &str,
        label: &str,
        module_id: &str,
        summary: &str,
        file_name: &str,
        content_type: &str,
        transfer_encoding: &str,
        byte_len: u64,
        payload: &str,
    ) {
        let prepared = match prepare_artifact_transfer(
            &self.snapshot,
            kind,
            label,
            module_id,
            summary,
            file_name,
            content_type,
            transfer_encoding,
            byte_len,
            payload,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.snapshot.last_error = error;
                self.snapshot.status = "Transfer send blocked by size or format limit.".to_string();
                self.dirty = true;
                return;
            }
        };

        if prepared.packets.is_empty() {
            self.snapshot.last_error = "Transfer did not produce any packets.".to_string();
            self.snapshot.status = "Transfer send failed before it started.".to_string();
            self.dirty = true;
            return;
        }

        let artifact = prepared.artifact.clone();
        if artifact.declared_byte_len() as usize > MAX_ARTIFACT_TOTAL_BYTES {
            self.record_outgoing_artifact_failure(
                artifact,
                "(size check)".to_string(),
                "Transfer too large for local LAN send.".to_string(),
            );
            self.snapshot.last_error = format!(
                "Transfer too large to send ({} bytes; limit {} bytes).",
                byte_len, MAX_ARTIFACT_TOTAL_BYTES
            );
            self.snapshot.status = "Transfer send blocked by size limit.".to_string();
            self.dirty = true;
            return;
        }

        let Some(connection) = self.connections.get_mut(connection_id) else {
            self.snapshot.last_error =
                "That connection is no longer active, so the transfer could not be sent."
                    .to_string();
            self.dirty = true;
            return;
        };
        let target_name = connection
            .peer_name
            .clone()
            .unwrap_or_else(|| connection.address.to_string());
        let target_id = connection.peer_id.clone().unwrap_or_default();
        let target_address = connection.address.to_string();
        match write_packets(&mut connection.stream, &prepared.packets) {
            Ok(()) => {
                self.record_outgoing_artifact_pending(
                    connection_id,
                    &target_id,
                    &target_name,
                    &target_address,
                    prepared,
                );
                self.snapshot.last_error.clear();
                self.snapshot.status = format!(
                    "Sent {} to {}. Waiting for delivery confirmation...",
                    if label.trim().is_empty() {
                        kind.trim()
                    } else {
                        label.trim()
                    },
                    target_name
                );
            }
            Err(error) => {
                self.record_outgoing_artifact_failure(
                    artifact,
                    target_name.clone(),
                    format!("Send failed: {error}"),
                );
                self.snapshot.last_error = format!("Could not send transfer: {error}");
                self.snapshot.status = "Transfer send failed.".to_string();
            }
        }
        self.dirty = true;
    }

    fn send_session_event(
        &mut self,
        connection_id: &str,
        scope_module_id: &str,
        session_id: &str,
        event_type: &str,
        label: &str,
        content_type: &str,
        payload_text: &str,
    ) {
        let mut event = SessionEventPayload {
            event_id: format!("evt-{}-{}", self.snapshot.device_id, now_unix_ms()),
            scope_module_id: scope_module_id.trim().to_string(),
            session_id: session_id.trim().to_string(),
            event_type: event_type.trim().to_string(),
            label: label.trim().to_string(),
            content_type: content_type.trim().to_string(),
            payload_text: payload_text.to_string(),
            sent_at_unix_ms: now_unix_ms(),
        };
        event.normalize();
        if let Err(error) = event.validate() {
            self.snapshot.last_error = error;
            self.snapshot.status =
                "Session event send blocked by size or format limit.".to_string();
            self.dirty = true;
            return;
        }

        let Some(connection) = self.connections.get_mut(connection_id) else {
            self.snapshot.last_error =
                "That connection is no longer active, so the session event could not be sent."
                    .to_string();
            self.dirty = true;
            return;
        };

        let packet = NetworkPacket::session_event(&self.snapshot, event);
        match write_packet(&mut connection.stream, &packet) {
            Ok(()) => {
                self.snapshot.last_error.clear();
                self.snapshot.status = format!(
                    "Sent session event to {}.",
                    connection
                        .peer_name
                        .clone()
                        .unwrap_or_else(|| connection.address.to_string())
                );
            }
            Err(error) => {
                self.snapshot.last_error = format!("Could not send session event: {error}");
                self.snapshot.status = "Session event send failed.".to_string();
            }
        }
        self.dirty = true;
    }

    fn record_outgoing_artifact_pending(
        &mut self,
        connection_id: &str,
        target_id: &str,
        target_name: &str,
        target_address: &str,
        prepared: PreparedArtifactTransfer,
    ) {
        let artifact = prepared.artifact;
        let content_type = artifact.normalized_content_type();
        let transfer_encoding = artifact.normalized_transfer_encoding().to_string();
        let byte_len = artifact.declared_byte_len();
        let chunk_count = artifact.normalized_chunk_count();
        let now = Instant::now();
        self.outgoing_artifacts.retain(|delivery| {
            delivery.artifact_id != artifact.artifact_id || delivery.connection_id != connection_id
        });
        self.outgoing_artifacts.insert(
            0,
            OutgoingArtifactDeliveryState {
                artifact_id: artifact.artifact_id,
                connection_id: connection_id.to_string(),
                to_device_id: target_id.to_string(),
                to_device_name: target_name.to_string(),
                to_address: target_address.to_string(),
                kind: artifact.kind,
                label: artifact.label,
                module_id: artifact.module_id,
                summary: artifact.summary,
                file_name: artifact.file_name,
                content_type,
                transfer_encoding,
                byte_len,
                chunk_count,
                status: "Waiting for delivery confirmation.".to_string(),
                attempts: 1,
                waiting_for_ack: true,
                packets: prepared.packets,
                last_attempt_at: now,
                updated_at: now,
            },
        );
        self.outgoing_artifacts
            .truncate(MAX_TRACKED_OUTGOING_ARTIFACTS);
    }

    fn record_outgoing_artifact_failure(
        &mut self,
        artifact: ArtifactPayload,
        target_name: String,
        status: String,
    ) {
        let content_type = artifact.normalized_content_type();
        let transfer_encoding = artifact.normalized_transfer_encoding().to_string();
        let byte_len = artifact.declared_byte_len();
        let chunk_count = artifact.normalized_chunk_count();
        let now = Instant::now();
        self.outgoing_artifacts.insert(
            0,
            OutgoingArtifactDeliveryState {
                artifact_id: artifact.artifact_id,
                connection_id: String::new(),
                to_device_id: String::new(),
                to_device_name: target_name,
                to_address: String::new(),
                kind: artifact.kind,
                label: artifact.label,
                module_id: artifact.module_id,
                summary: artifact.summary,
                file_name: artifact.file_name,
                content_type,
                transfer_encoding,
                byte_len,
                chunk_count,
                status,
                attempts: 1,
                waiting_for_ack: false,
                packets: Vec::new(),
                last_attempt_at: now,
                updated_at: now,
            },
        );
        self.outgoing_artifacts
            .truncate(MAX_TRACKED_OUTGOING_ARTIFACTS);
    }

    fn fail_pending_artifacts_for_connection(&mut self, connection_id: &str, reason: &str) {
        let now = Instant::now();
        let mut changed = false;
        for delivery in &mut self.outgoing_artifacts {
            if delivery.connection_id == connection_id && delivery.waiting_for_ack {
                delivery.waiting_for_ack = false;
                delivery.packets.clear();
                delivery.status = reason.to_string();
                delivery.updated_at = now;
                changed = true;
            }
        }
        if changed {
            self.dirty = true;
        }
    }

    fn send_artifact_ack(
        &mut self,
        connection_id: &str,
        artifact_id: &str,
        accepted: bool,
        duplicate: bool,
        message: &str,
    ) {
        let packet =
            NetworkPacket::artifact_ack(&self.snapshot, artifact_id, accepted, duplicate, message);
        let Some(connection) = self.connections.get_mut(connection_id) else {
            return;
        };
        if let Err(error) = write_packet(&mut connection.stream, &packet) {
            self.snapshot.last_error = format!("Could not send transfer acknowledgement: {error}");
            self.dirty = true;
        }
    }

    fn handle_artifact_ack(
        &mut self,
        packet: &NetworkPacket,
        ack: ArtifactAckPayload,
        address: SocketAddr,
    ) {
        let now = Instant::now();
        let from_name = if packet.device_name.trim().is_empty() {
            address.to_string()
        } else {
            packet.device_name.clone()
        };
        let mut matched = false;
        for delivery in &mut self.outgoing_artifacts {
            if delivery.artifact_id == ack.artifact_id {
                delivery.waiting_for_ack = false;
                delivery.updated_at = now;
                delivery.packets.clear();
                delivery.to_device_id = packet.device_id.clone();
                delivery.to_device_name = from_name.clone();
                delivery.to_address = address.to_string();
                delivery.status = if ack.accepted && ack.duplicate {
                    if ack.message.trim().is_empty() {
                        "Delivered (peer already had it).".to_string()
                    } else {
                        format!("Delivered (duplicate-safe): {}", ack.message.trim())
                    }
                } else if ack.accepted {
                    if ack.message.trim().is_empty() {
                        "Delivered.".to_string()
                    } else {
                        format!("Delivered: {}", ack.message.trim())
                    }
                } else if ack.message.trim().is_empty() {
                    "Rejected by peer.".to_string()
                } else {
                    format!("Rejected: {}", ack.message.trim())
                };
                matched = true;
                break;
            }
        }

        if matched {
            self.snapshot.last_error.clear();
            self.snapshot.status = if ack.accepted {
                format!("Transfer confirmed by {}.", from_name)
            } else {
                format!("Transfer rejected by {}.", from_name)
            };
            self.dirty = true;
        }
    }

    fn poll_pending_artifact_acks(&mut self) {
        let now = Instant::now();
        let mut retries = Vec::new();

        for delivery in &mut self.outgoing_artifacts {
            if !delivery.waiting_for_ack
                || now.duration_since(delivery.last_attempt_at) < ARTIFACT_ACK_TIMEOUT
            {
                continue;
            }

            if delivery.attempts >= MAX_ARTIFACT_SEND_ATTEMPTS {
                delivery.waiting_for_ack = false;
                delivery.packets.clear();
                delivery.status = "No delivery confirmation received.".to_string();
                delivery.updated_at = now;
                self.dirty = true;
                continue;
            }

            retries.push((
                delivery.artifact_id.clone(),
                delivery.connection_id.clone(),
                delivery.packets.clone(),
                delivery.attempts + 1,
            ));
        }

        let mut failed_connections = Vec::new();
        for (artifact_id, connection_id, packets, next_attempt) in retries {
            let Some(connection) = self.connections.get_mut(&connection_id) else {
                if let Some(delivery) = self
                    .outgoing_artifacts
                    .iter_mut()
                    .find(|delivery| delivery.artifact_id == artifact_id)
                {
                    delivery.waiting_for_ack = false;
                    delivery.packets.clear();
                    delivery.status = "Connection missing before retry.".to_string();
                    delivery.updated_at = now;
                    self.dirty = true;
                }
                continue;
            };

            match write_packets(&mut connection.stream, &packets) {
                Ok(()) => {
                    if let Some(delivery) = self
                        .outgoing_artifacts
                        .iter_mut()
                        .find(|delivery| delivery.artifact_id == artifact_id)
                    {
                        delivery.attempts = next_attempt;
                        delivery.last_attempt_at = now;
                        delivery.updated_at = now;
                        delivery.status = format!(
                            "Retrying delivery ({}/{})...",
                            next_attempt, MAX_ARTIFACT_SEND_ATTEMPTS
                        );
                        self.dirty = true;
                    }
                }
                Err(error) => {
                    if let Some(delivery) = self
                        .outgoing_artifacts
                        .iter_mut()
                        .find(|delivery| delivery.artifact_id == artifact_id)
                    {
                        delivery.waiting_for_ack = false;
                        delivery.packets.clear();
                        delivery.attempts = next_attempt;
                        delivery.updated_at = now;
                        delivery.status = format!("Retry failed: {error}");
                        self.dirty = true;
                    }
                    failed_connections.push(connection_id);
                }
            }
        }

        for connection_id in failed_connections {
            self.disconnect_connection(&connection_id);
        }
    }

    fn prune_recent_artifact_ids(&mut self) {
        let now = Instant::now();
        self.seen_artifact_ids
            .retain(|_, received_at| now.duration_since(*received_at) <= RECENT_ARTIFACT_ID_TTL);
    }

    fn prune_recent_session_event_ids(&mut self) {
        let now = Instant::now();
        self.seen_session_event_ids.retain(|_, received_at| {
            now.duration_since(*received_at) <= RECENT_SESSION_EVENT_ID_TTL
        });
    }

    fn prune_incoming_artifact_assemblies(&mut self) {
        let now = Instant::now();
        let before = self.incoming_artifact_assemblies.len();
        self.incoming_artifact_assemblies
            .retain(|_, assembly| now.duration_since(assembly.updated_at) <= PARTIAL_ARTIFACT_TTL);
        if self.incoming_artifact_assemblies.len() != before {
            self.dirty = true;
        }
    }

    fn prune_outgoing_artifacts(&mut self) {
        let now = Instant::now();
        let before = self.outgoing_artifacts.len();
        self.outgoing_artifacts.retain(|delivery| {
            delivery.waiting_for_ack
                || now.duration_since(delivery.updated_at) <= OUTGOING_ARTIFACT_TTL
        });
        if self.outgoing_artifacts.len() > MAX_TRACKED_OUTGOING_ARTIFACTS {
            self.outgoing_artifacts
                .truncate(MAX_TRACKED_OUTGOING_ARTIFACTS);
        }
        if self.outgoing_artifacts.len() != before {
            self.dirty = true;
        }
    }

    fn poll_tcp_accepts(&mut self) {
        let Some(listener) = self
            .tcp_listener
            .as_ref()
            .and_then(|listener| listener.try_clone().ok())
        else {
            return;
        };

        loop {
            match listener.accept() {
                Ok((mut stream, addr)) => {
                    let _ = stream.set_nonblocking(true);
                    let connection_id = self.next_connection_id();
                    let _ = self.send_hello(&mut stream);
                    self.connections.insert(
                        connection_id.clone(),
                        LiveConnection {
                            connection_id,
                            stream,
                            address: addr,
                            peer_id: None,
                            peer_name: None,
                            inbound: true,
                            connected_at: Instant::now(),
                            read_buffer: Vec::new(),
                            last_status: None,
                            last_status_at: None,
                        },
                    );
                    self.snapshot.status = format!(
                        "Accepted a new connection from {}. Waiting for identity...",
                        addr
                    );
                    self.dirty = true;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    self.snapshot.last_error = format!("Accept failed: {error}");
                    self.dirty = true;
                    break;
                }
            }
        }
    }

    fn poll_connections(&mut self) {
        let connection_ids = self.connections.keys().cloned().collect::<Vec<_>>();
        let mut closed = Vec::new();

        for connection_id in connection_ids {
            let mut parsed_lines = Vec::new();
            let mut should_close = false;

            {
                let Some(connection) = self.connections.get_mut(&connection_id) else {
                    continue;
                };

                let mut temp = [0u8; 2048];
                loop {
                    match connection.stream.read(&mut temp) {
                        Ok(0) => {
                            should_close = true;
                            break;
                        }
                        Ok(len) => {
                            connection.read_buffer.extend_from_slice(&temp[..len]);
                            while let Some(newline_index) = connection
                                .read_buffer
                                .iter()
                                .position(|byte| *byte == b'\n')
                            {
                                let line = connection
                                    .read_buffer
                                    .drain(..=newline_index)
                                    .collect::<Vec<_>>();
                                parsed_lines.push(line[..line.len().saturating_sub(1)].to_vec());
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(_) => {
                            should_close = true;
                            break;
                        }
                    }
                }
            }

            for line in parsed_lines {
                let address = self
                    .connections
                    .get(&connection_id)
                    .map(|connection| connection.address)
                    .unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
                self.handle_connection_line(&connection_id, address, &line);
            }

            if should_close {
                closed.push(connection_id);
            }
        }

        for connection_id in closed {
            if let Some(connection) = self.connections.remove(&connection_id) {
                self.fail_pending_artifacts_for_connection(
                    &connection_id,
                    "Connection dropped before delivery confirmation.",
                );
                let _ = connection.stream.shutdown(Shutdown::Both);
                self.snapshot.status = format!(
                    "Connection closed: {}.",
                    connection
                        .peer_name
                        .clone()
                        .unwrap_or_else(|| connection.address.to_string())
                );
                self.dirty = true;
            }
        }
    }

    fn handle_connection_line(&mut self, connection_id: &str, address: SocketAddr, bytes: &[u8]) {
        let Ok(packet) = serde_json::from_slice::<NetworkPacket>(bytes) else {
            return;
        };
        if let Some(reason) = packet.compatibility_error() {
            self.record_protocol_notice(format!("{} From {}.", reason, address));
            self.snapshot.status = "Rejected an incompatible LAN peer.".to_string();
            self.disconnect_connection(connection_id);
            return;
        }

        let inbound = self
            .connections
            .get(connection_id)
            .map(|connection| connection.inbound)
            .unwrap_or(false);

        if self.blocked_peers.contains_key(&packet.device_id) {
            if let Some(blocked) = self.blocked_peers.get_mut(&packet.device_id) {
                blocked.device_name = packet.device_name.clone();
                blocked.address = address.to_string();
                blocked.last_seen = Some(Instant::now());
            }
            self.snapshot.status = format!("Blocked {} from connecting.", packet.device_name);
            self.disconnect_connection(connection_id);
            self.dirty = true;
            return;
        }

        self.note_trusted_peer_activity(
            &packet.device_id,
            &packet.device_name,
            address.to_string(),
        );

        if inbound
            && !self.snapshot.allow_unknown_devices
            && !self.approved_peers.contains(&packet.device_id)
        {
            self.pending_requests
                .entry(packet.device_id.clone())
                .and_modify(|request| {
                    request.device_name = packet.device_name.clone();
                    request.address = address.to_string();
                    request.requested_at = Instant::now();
                })
                .or_insert_with(|| PendingPeerRequestState {
                    device_id: packet.device_id.clone(),
                    device_name: packet.device_name.clone(),
                    address: address.to_string(),
                    requested_at: Instant::now(),
                });
            self.snapshot.status = format!(
                "Unknown device {} is requesting connection.",
                packet.device_name
            );
            self.disconnect_connection(connection_id);
            self.dirty = true;
            return;
        }

        self.approved_peers.insert(packet.device_id.clone());
        self.pending_requests.remove(&packet.device_id);

        if let Some(connection) = self.connections.get_mut(connection_id) {
            connection.peer_id = Some(packet.device_id.clone());
            connection.peer_name = Some(packet.device_name.clone());
        }

        if let Some(port) = packet.tcp_port {
            self.peers.insert(
                packet.device_id.clone(),
                KnownPeer {
                    device_id: packet.device_id.clone(),
                    device_name: packet.device_name.clone(),
                    address: SocketAddr::new(address.ip(), port),
                    host_port: port,
                    last_seen: Instant::now(),
                },
            );
        }

        if let Some(status) = packet.status.clone() {
            self.apply_status_to_connection(connection_id, status);
        }

        if packet.kind == "handoff" {
            if let Some(handoff) = packet.handoff {
                self.received_handoffs.insert(
                    0,
                    ReceivedHandoffState {
                        handoff_id: handoff.handoff_id,
                        from_device_id: packet.device_id,
                        from_device_name: packet.device_name,
                        from_address: address.to_string(),
                        title: handoff.title,
                        body: handoff.body,
                        received_at: Instant::now(),
                    },
                );
                self.received_handoffs.truncate(24);
                self.snapshot.status =
                    "Received a handoff from another Chatty-EDU instance.".to_string();
            }
        } else if packet.kind == "artifact_ack" {
            if let Some(ack) = packet.artifact_ack.clone() {
                self.handle_artifact_ack(&packet, ack, address);
            }
        } else if packet.kind == "artifact" {
            if let Some(artifact) = packet.artifact {
                let artifact_id = artifact.artifact_id.clone();
                if self.seen_artifact_ids.contains_key(&artifact.artifact_id) {
                    self.send_artifact_ack(
                        connection_id,
                        &artifact.artifact_id,
                        true,
                        true,
                        "Already received earlier in this session.",
                    );
                    self.snapshot.status =
                        "Ignored a duplicate shared transfer from another Chatty-EDU instance."
                            .to_string();
                    self.dirty = true;
                    return;
                }

                match self.ingest_artifact_chunk(
                    connection_id,
                    &packet.device_id,
                    &packet.device_name,
                    &address.to_string(),
                    artifact,
                ) {
                    Ok(Some(received)) => {
                        self.seen_artifact_ids
                            .insert(received.artifact_id.clone(), Instant::now());
                        let artifact_id = received.artifact_id.clone();
                        self.received_artifacts.insert(0, received);
                        self.received_artifacts.truncate(MAX_RECENT_ARTIFACTS);
                        self.send_artifact_ack(
                            connection_id,
                            &artifact_id,
                            true,
                            false,
                            "Saved to the local transfer inbox.",
                        );
                        self.snapshot.status =
                            "Received a shared transfer from another Chatty-EDU instance."
                                .to_string();
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.send_artifact_ack(connection_id, &artifact_id, false, false, &error);
                        self.snapshot.last_error = error;
                        self.snapshot.status =
                            "Rejected an invalid or unsupported shared transfer.".to_string();
                        self.dirty = true;
                        return;
                    }
                }
            }
        } else if packet.kind == "session_event" {
            if let Some(mut event) = packet.session_event.clone() {
                event.normalize();
                match event.validate() {
                    Ok(()) => {
                        if !self.seen_session_event_ids.contains_key(&event.event_id) {
                            self.seen_session_event_ids
                                .insert(event.event_id.clone(), Instant::now());
                            self.received_session_events.insert(
                                0,
                                ReceivedSessionEventState {
                                    event_id: event.event_id,
                                    from_device_id: packet.device_id.clone(),
                                    from_device_name: packet.device_name.clone(),
                                    from_address: address.to_string(),
                                    scope_module_id: event.scope_module_id,
                                    session_id: event.session_id,
                                    event_type: event.event_type,
                                    label: event.label,
                                    content_type: event.content_type,
                                    payload_text: event.payload_text,
                                    received_at_unix_ms: now_unix_ms().max(0) as u64,
                                    received_at: Instant::now(),
                                },
                            );
                            self.received_session_events
                                .truncate(MAX_RECENT_SESSION_EVENTS);
                            self.snapshot.status =
                                "Received a session event from another Chatty-EDU instance."
                                    .to_string();
                        }
                    }
                    Err(error) => {
                        self.snapshot.last_error = error;
                        self.snapshot.status =
                            "Rejected an invalid shared session event.".to_string();
                    }
                }
            }
        }

        self.snapshot.last_error.clear();
        self.dirty = true;
    }

    fn ingest_artifact_chunk(
        &mut self,
        _connection_id: &str,
        from_device_id: &str,
        from_device_name: &str,
        from_address: &str,
        artifact: ArtifactPayload,
    ) -> Result<Option<ReceivedArtifactState>, String> {
        artifact.validate_chunk_metadata()?;

        if artifact.declared_byte_len() as usize > MAX_ARTIFACT_TOTAL_BYTES {
            return Err(format!(
                "Transfer too large for this local inbox ({} bytes; limit {} bytes).",
                artifact.declared_byte_len(),
                MAX_ARTIFACT_TOTAL_BYTES
            ));
        }

        if !self
            .incoming_artifact_assemblies
            .contains_key(&artifact.artifact_id)
            && self.incoming_artifact_assemblies.len() >= MAX_INCOMING_ARTIFACT_ASSEMBLIES
        {
            return Err("Too many in-flight transfers. Please retry in a moment.".to_string());
        }

        let artifact_id = artifact.artifact_id.clone();
        let expected_encoded_len = artifact.expected_encoded_len()?;
        let fragment_len = artifact.fragment().as_bytes().len();
        let chunk_count = artifact.normalized_chunk_count();
        let declared_byte_len = artifact.declared_byte_len();
        let now = Instant::now();

        let assembly = self
            .incoming_artifact_assemblies
            .entry(artifact_id.clone())
            .or_insert_with(|| IncomingArtifactAssemblyState {
                artifact_id: artifact_id.clone(),
                from_device_id: from_device_id.to_string(),
                from_device_name: from_device_name.to_string(),
                from_address: from_address.to_string(),
                kind: artifact.kind.clone(),
                label: artifact.label.clone(),
                module_id: artifact.module_id.clone(),
                summary: artifact.summary.clone(),
                file_name: artifact.file_name.clone(),
                content_type: artifact.normalized_content_type(),
                transfer_encoding: artifact.normalized_transfer_encoding().to_string(),
                byte_len: declared_byte_len,
                chunk_count,
                chunks: vec![None; chunk_count as usize],
                encoded_bytes_received: 0,
                received_chunks: 0,
                updated_at: now,
            });

        if assembly.from_device_id != from_device_id
            || assembly.transfer_encoding != artifact.normalized_transfer_encoding()
            || assembly.byte_len != declared_byte_len
            || assembly.chunk_count != chunk_count
            || assembly.kind != artifact.kind
            || assembly.file_name != artifact.file_name
        {
            return Err("Transfer metadata changed mid-stream. Please retry.".to_string());
        }

        let chunk_slot = assembly
            .chunks
            .get_mut(artifact.chunk_index as usize)
            .ok_or_else(|| "Chunk index is outside the declared transfer range.".to_string())?;

        if chunk_slot.is_none() {
            if assembly.encoded_bytes_received + fragment_len > expected_encoded_len {
                return Err("Transfer exceeded expected encoded size.".to_string());
            }
            assembly.encoded_bytes_received += fragment_len;
            assembly.received_chunks += 1;
            *chunk_slot = Some(artifact.fragment().to_string());
        }
        assembly.updated_at = now;

        if assembly.received_chunks < assembly.chunk_count {
            return Ok(None);
        }

        let completed = self
            .incoming_artifact_assemblies
            .remove(&artifact_id)
            .ok_or_else(|| "Transfer assembly disappeared before completion.".to_string())?;
        let payload = completed
            .chunks
            .into_iter()
            .map(|chunk| chunk.unwrap_or_default())
            .collect::<Vec<_>>()
            .join("");

        let (text, data_base64) = match completed.transfer_encoding.as_str() {
            ARTIFACT_ENCODING_UTF8 => {
                if payload.as_bytes().len() != completed.byte_len as usize {
                    return Err(format!(
                        "Transfer size mismatch (expected {} bytes, received {}).",
                        completed.byte_len,
                        payload.as_bytes().len()
                    ));
                }
                (payload, String::new())
            }
            ARTIFACT_ENCODING_BASE64 => {
                let bytes = BASE64
                    .decode(payload.as_bytes())
                    .map_err(|error| format!("Could not decode binary transfer: {error}"))?;
                if bytes.len() != completed.byte_len as usize {
                    return Err(format!(
                        "Binary transfer size mismatch (expected {} bytes, decoded {}).",
                        completed.byte_len,
                        bytes.len()
                    ));
                }
                (String::new(), payload)
            }
            other => {
                return Err(format!("Unsupported transfer encoding `{other}`."));
            }
        };

        Ok(Some(ReceivedArtifactState {
            artifact_id: completed.artifact_id,
            from_device_id: completed.from_device_id,
            from_device_name: completed.from_device_name,
            from_address: completed.from_address,
            kind: completed.kind,
            label: completed.label,
            module_id: completed.module_id,
            summary: completed.summary,
            file_name: completed.file_name,
            content_type: completed.content_type,
            transfer_encoding: completed.transfer_encoding,
            byte_len: completed.byte_len,
            chunk_count: completed.chunk_count,
            text,
            data_base64,
            received_at: Instant::now(),
        }))
    }

    fn apply_status_to_connection(&mut self, connection_id: &str, status: StatusPayload) {
        if let Some(connection) = self.connections.get_mut(connection_id) {
            connection.last_status = Some(status);
            connection.last_status_at = Some(Instant::now());
        }
    }

    fn send_hello(&self, stream: &mut TcpStream) -> Result<(), String> {
        let packet = NetworkPacket::hello(&self.snapshot);
        write_packet(stream, &packet).map_err(|error| format!("Could not send hello: {error}"))
    }

    fn send_packet_to_all(&mut self, packet: &NetworkPacket) -> usize {
        let connection_ids = self.connections.keys().cloned().collect::<Vec<_>>();
        let mut sent = 0usize;
        let mut failed = Vec::new();

        for connection_id in connection_ids {
            let Some(connection) = self.connections.get_mut(&connection_id) else {
                continue;
            };
            match write_packet(&mut connection.stream, packet) {
                Ok(()) => sent += 1,
                Err(_) => failed.push(connection_id),
            }
        }

        for connection_id in failed {
            if let Some(connection) = self.connections.remove(&connection_id) {
                let _ = connection.stream.shutdown(Shutdown::Both);
            }
        }

        sent
    }

    fn prune_stale_peers(&mut self) {
        let now = Instant::now();
        let before = self.peers.len();
        self.peers
            .retain(|_, peer| now.duration_since(peer.last_seen) <= PEER_TTL);
        if self.peers.len() != before {
            self.dirty = true;
        }
    }

    fn push_snapshot(&mut self) {
        let connected_by_peer = self
            .connections
            .values()
            .filter_map(|connection| {
                connection
                    .peer_id
                    .as_ref()
                    .map(|peer_id| (peer_id.clone(), connection.connection_id.clone()))
            })
            .collect::<HashMap<_, _>>();

        let mut discovered = self
            .peers
            .values()
            .cloned()
            .map(|peer| DiscoveredPeer {
                connected_connection_id: connected_by_peer.get(&peer.device_id).cloned(),
                device_id: peer.device_id,
                device_name: peer.device_name,
                address: peer.address.ip().to_string(),
                host_port: peer.host_port,
                last_seen_secs_ago: peer.last_seen.elapsed().as_secs(),
            })
            .collect::<Vec<_>>();
        discovered.sort_by(|left, right| left.device_name.cmp(&right.device_name));

        let mut connected = self
            .connections
            .values()
            .map(|connection| ConnectedPeer {
                connection_id: connection.connection_id.clone(),
                device_id: connection.peer_id.clone().unwrap_or_default(),
                device_name: connection
                    .peer_name
                    .clone()
                    .unwrap_or_else(|| connection.address.to_string()),
                address: connection.address.to_string(),
                inbound: connection.inbound,
                connected_secs: connection.connected_at.elapsed().as_secs(),
                status_summary: connection
                    .last_status
                    .as_ref()
                    .map(StatusPayload::summary)
                    .unwrap_or_else(|| "No shared status yet.".to_string()),
                status_age_secs: connection.last_status_at.map(|ts| ts.elapsed().as_secs()),
            })
            .collect::<Vec<_>>();
        connected.sort_by(|left, right| left.device_name.cmp(&right.device_name));

        let mut blocked = self
            .blocked_peers
            .values()
            .cloned()
            .map(|peer| BlockedPeer {
                device_id: peer.device_id,
                device_name: peer.device_name,
                address: peer.address,
                last_seen_secs_ago: peer.last_seen.map(|ts| ts.elapsed().as_secs()),
            })
            .collect::<Vec<_>>();
        blocked.sort_by(|left, right| left.device_name.cmp(&right.device_name));

        let mut trusted = self
            .trusted_peers
            .values()
            .cloned()
            .map(|peer| TrustedPeer {
                device_id: peer.device_id,
                device_name: peer.device_name,
                address: peer.address,
                last_seen_secs_ago: peer.last_seen.map(|ts| ts.elapsed().as_secs()),
            })
            .collect::<Vec<_>>();
        trusted.sort_by(|left, right| left.device_name.cmp(&right.device_name));

        let mut pending = self
            .pending_requests
            .values()
            .cloned()
            .map(|request| PendingPeerRequest {
                device_id: request.device_id,
                device_name: request.device_name,
                address: request.address,
                requested_secs_ago: request.requested_at.elapsed().as_secs(),
            })
            .collect::<Vec<_>>();
        pending.sort_by(|left, right| left.device_name.cmp(&right.device_name));

        self.snapshot.discovered_peers = discovered;
        self.snapshot.connected_peers = connected;
        self.snapshot.trusted_peers = trusted;
        self.snapshot.blocked_peers = blocked;
        self.snapshot.pending_requests = pending;
        self.snapshot.received_handoffs = self
            .received_handoffs
            .iter()
            .map(|handoff| ReceivedHandoff {
                handoff_id: handoff.handoff_id.clone(),
                from_device_id: handoff.from_device_id.clone(),
                from_device_name: handoff.from_device_name.clone(),
                from_address: handoff.from_address.clone(),
                title: handoff.title.clone(),
                body: handoff.body.clone(),
                received_secs_ago: handoff.received_at.elapsed().as_secs(),
            })
            .collect();
        self.snapshot.received_artifacts = self
            .received_artifacts
            .iter()
            .map(|artifact| ReceivedArtifact {
                artifact_id: artifact.artifact_id.clone(),
                from_device_id: artifact.from_device_id.clone(),
                from_device_name: artifact.from_device_name.clone(),
                from_address: artifact.from_address.clone(),
                kind: artifact.kind.clone(),
                label: artifact.label.clone(),
                module_id: artifact.module_id.clone(),
                summary: artifact.summary.clone(),
                file_name: artifact.file_name.clone(),
                content_type: artifact.content_type.clone(),
                transfer_encoding: artifact.transfer_encoding.clone(),
                byte_len: artifact.byte_len,
                chunk_count: artifact.chunk_count,
                text: artifact.text.clone(),
                data_base64: artifact.data_base64.clone(),
                received_secs_ago: artifact.received_at.elapsed().as_secs(),
            })
            .collect();
        self.snapshot.received_session_events = self
            .received_session_events
            .iter()
            .map(|event| ReceivedSessionEvent {
                event_id: event.event_id.clone(),
                from_device_id: event.from_device_id.clone(),
                from_device_name: event.from_device_name.clone(),
                from_address: event.from_address.clone(),
                scope_module_id: event.scope_module_id.clone(),
                session_id: event.session_id.clone(),
                event_type: event.event_type.clone(),
                label: event.label.clone(),
                content_type: event.content_type.clone(),
                payload_text: event.payload_text.clone(),
                received_at_unix_ms: event.received_at_unix_ms,
                received_secs_ago: event.received_at.elapsed().as_secs(),
            })
            .collect();
        self.snapshot.outgoing_artifacts = self
            .outgoing_artifacts
            .iter()
            .map(|artifact| OutgoingArtifactDelivery {
                artifact_id: artifact.artifact_id.clone(),
                to_device_id: artifact.to_device_id.clone(),
                to_device_name: artifact.to_device_name.clone(),
                to_address: artifact.to_address.clone(),
                kind: artifact.kind.clone(),
                label: artifact.label.clone(),
                module_id: artifact.module_id.clone(),
                summary: artifact.summary.clone(),
                file_name: artifact.file_name.clone(),
                content_type: artifact.content_type.clone(),
                transfer_encoding: artifact.transfer_encoding.clone(),
                byte_len: artifact.byte_len,
                chunk_count: artifact.chunk_count,
                status: artifact.status.clone(),
                attempts: artifact.attempts,
                waiting_for_ack: artifact.waiting_for_ack,
                updated_secs_ago: artifact.updated_at.elapsed().as_secs(),
            })
            .collect();
        self.last_snapshot_sent = Instant::now();
        self.dirty = false;
        let _ = self
            .event_tx
            .send(NetworkEvent::Snapshot(self.snapshot.clone()));
    }

    fn next_connection_id(&mut self) -> String {
        let next = self.next_connection_id;
        self.next_connection_id += 1;
        format!("conn-{next}")
    }

    fn disconnect_peer_id(&mut self, device_id: &str) {
        let to_close = self
            .connections
            .iter()
            .filter_map(|(connection_id, connection)| {
                (connection.peer_id.as_deref() == Some(device_id)).then_some(connection_id.clone())
            })
            .collect::<Vec<_>>();
        for connection_id in to_close {
            self.disconnect_connection(&connection_id);
        }
    }

    fn shutdown_all_connections(&mut self) {
        for (_, connection) in self.connections.drain() {
            let _ = connection.stream.shutdown(Shutdown::Both);
        }
    }

    fn shutdown_all(&mut self) {
        self.discovery_listener = None;
        self.tcp_listener = None;
        self.shutdown_all_connections();
    }
}

fn make_device_name() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Unknown device".to_string());
    format!("Chatty-EDU on {} ({})", host, std::process::id())
}

fn make_device_id() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "device".to_string())
        .to_ascii_lowercase()
        .replace(' ', "-");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("chatty-edu-{host}-{}-{now}", std::process::id())
}

fn sanitize_device_id(input: &str) -> Option<String> {
    let trimmed = input.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn infer_text_content_type(kind: &str, file_name: &str) -> String {
    let kind = kind.trim().to_ascii_lowercase();
    let file_name = file_name.trim().to_ascii_lowercase();
    if kind.ends_with("_json") || file_name.ends_with(".json") {
        CONTENT_TYPE_JSON.to_string()
    } else if kind.ends_with("_markdown")
        || kind.ends_with("_md")
        || file_name.ends_with(".md")
        || file_name.ends_with(".markdown")
    {
        CONTENT_TYPE_MARKDOWN.to_string()
    } else {
        CONTENT_TYPE_TEXT.to_string()
    }
}

fn expected_encoded_len(byte_len: u64, transfer_encoding: &str) -> Result<usize, String> {
    if byte_len as usize > MAX_ARTIFACT_TOTAL_BYTES {
        return Err(format!(
            "Transfer too large ({} bytes; limit {} bytes).",
            byte_len, MAX_ARTIFACT_TOTAL_BYTES
        ));
    }
    let encoded = match transfer_encoding {
        ARTIFACT_ENCODING_UTF8 => byte_len as usize,
        ARTIFACT_ENCODING_BASE64 => byte_len
            .checked_add(2)
            .ok_or_else(|| "Encoded transfer length overflowed.".to_string())?
            .div_ceil(3)
            .checked_mul(4)
            .ok_or_else(|| "Encoded transfer length overflowed.".to_string())?
            as usize,
        other => return Err(format!("Unsupported transfer encoding `{other}`.")),
    };
    Ok(encoded)
}

fn max_encoded_payload_storage(byte_len: u64) -> usize {
    ((byte_len as usize).saturating_mul(4) / 3).saturating_add(16)
}

fn split_payload_chunks(payload: &str, max_chunk_bytes: usize) -> Vec<String> {
    if payload.is_empty() {
        return vec![String::new()];
    }

    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < payload.len() {
        let remaining = payload.len() - start;
        let mut end = start + remaining.min(max_chunk_bytes);
        while end > start && !payload.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            end = payload[start..]
                .char_indices()
                .nth(1)
                .map(|(idx, _)| start + idx)
                .unwrap_or(payload.len());
        }
        chunks.push(payload[start..end].to_string());
        start = end;
    }
    chunks
}

fn prepare_artifact_transfer(
    snapshot: &NetworkSnapshot,
    kind: &str,
    label: &str,
    module_id: &str,
    summary: &str,
    file_name: &str,
    content_type: &str,
    transfer_encoding: &str,
    byte_len: u64,
    payload: &str,
) -> Result<PreparedArtifactTransfer, String> {
    let normalized_encoding = if transfer_encoding.trim().is_empty() {
        ARTIFACT_ENCODING_UTF8.to_string()
    } else {
        transfer_encoding.trim().to_string()
    };
    let declared_byte_len = if byte_len == 0 && normalized_encoding == ARTIFACT_ENCODING_UTF8 {
        payload.as_bytes().len() as u64
    } else {
        byte_len
    };
    let expected_len = expected_encoded_len(declared_byte_len, &normalized_encoding)?;
    let actual_len = payload.as_bytes().len();
    if actual_len != expected_len {
        return Err(format!(
            "Transfer payload length mismatch (expected encoded {} bytes, got {}).",
            expected_len, actual_len
        ));
    }

    let chunks = split_payload_chunks(payload, MAX_ARTIFACT_CHUNK_BYTES);
    if chunks.len() > u32::MAX as usize {
        return Err("Transfer produced too many chunks for LAN transport.".to_string());
    }
    let sent_at_unix_ms = now_unix_ms();
    let artifact_id = format!("artifact-{}-{}", snapshot.device_id, sent_at_unix_ms);
    let chunk_count = chunks.len().max(1) as u32;
    let normalized_content_type = if content_type.trim().is_empty() {
        if normalized_encoding == ARTIFACT_ENCODING_BASE64 {
            CONTENT_TYPE_BINARY.to_string()
        } else {
            infer_text_content_type(kind, file_name)
        }
    } else {
        content_type.trim().to_string()
    };

    let artifact = ArtifactPayload {
        artifact_id: artifact_id.clone(),
        kind: kind.trim().to_string(),
        label: label.trim().to_string(),
        module_id: module_id.trim().to_string(),
        summary: summary.trim().to_string(),
        file_name: file_name.trim().to_string(),
        content_type: normalized_content_type,
        transfer_encoding: normalized_encoding.clone(),
        byte_len: declared_byte_len,
        chunk_index: 0,
        chunk_count,
        text: String::new(),
        sent_at_unix_ms,
    };

    let packets = chunks
        .into_iter()
        .enumerate()
        .map(|(chunk_index, fragment)| {
            let mut chunk = artifact.clone();
            chunk.chunk_index = chunk_index as u32;
            chunk.text = fragment;
            NetworkPacket::artifact(snapshot, chunk)
        })
        .collect::<Vec<_>>();

    Ok(PreparedArtifactTransfer { artifact, packets })
}

fn write_packet(stream: &mut TcpStream, packet: &NetworkPacket) -> Result<(), String> {
    let mut bytes =
        serde_json::to_vec(packet).map_err(|error| format!("Could not encode packet: {error}"))?;
    bytes.push(b'\n');
    stream
        .write_all(&bytes)
        .map_err(|error| format!("Could not write packet: {error}"))
}

fn write_packets(stream: &mut TcpStream, packets: &[NetworkPacket]) -> Result<(), String> {
    for packet in packets {
        write_packet(stream, packet)?;
    }
    Ok(())
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
