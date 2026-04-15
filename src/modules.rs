use crate::module_host::ModuleVisualLoad;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn default_roles() -> Vec<String> {
    vec!["teacher".to_string(), "student".to_string()]
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ModuleNetworkFeature {
    SharedStatePublish,
    SharedStateReceive,
    WorkflowBundleSend,
    WorkflowBundleReceive,
    PackSend,
    PackReceive,
    LukewarmContextPublish,
    LukewarmContextReceive,
    RoomAware,
    Multiplayer,
    HostAuthoritative,
}

impl ModuleNetworkFeature {
    pub fn label(self) -> &'static str {
        match self {
            Self::SharedStatePublish => "Shared state out",
            Self::SharedStateReceive => "Shared state in",
            Self::WorkflowBundleSend => "Workflow bundles out",
            Self::WorkflowBundleReceive => "Workflow bundles in",
            Self::PackSend => "Packs out",
            Self::PackReceive => "Packs in",
            Self::LukewarmContextPublish => "Luke warm out",
            Self::LukewarmContextReceive => "Luke warm in",
            Self::RoomAware => "Room-aware",
            Self::Multiplayer => "Multiplayer",
            Self::HostAuthoritative => "Host-authoritative",
        }
    }
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ModuleAssetDirection {
    Incoming,
    Outgoing,
    #[default]
    InOut,
}

impl ModuleAssetDirection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Incoming => "Incoming",
            Self::Outgoing => "Outgoing",
            Self::InOut => "In + out",
        }
    }

    pub fn supports_receive(self) -> bool {
        matches!(self, Self::Incoming | Self::InOut)
    }

    #[allow(dead_code)]
    pub fn supports_send(self) -> bool {
        matches!(self, Self::Outgoing | Self::InOut)
    }

    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::InOut, _) | (_, Self::InOut) => Self::InOut,
            (Self::Incoming, Self::Outgoing) | (Self::Outgoing, Self::Incoming) => Self::InOut,
            (_, incoming) => incoming,
        }
    }
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ModuleAssetDeliveryMode {
    InboxOnly,
    #[default]
    BridgeInbox,
}

impl ModuleAssetDeliveryMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::InboxOnly => "Inbox only",
            Self::BridgeInbox => "Bridge inbox",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleNetworkAssetLane {
    #[serde(default)]
    pub lane_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub direction: ModuleAssetDirection,
    #[serde(default)]
    pub delivery_mode: ModuleAssetDeliveryMode,
    #[serde(default)]
    pub artifact_kinds: Vec<String>,
    #[serde(default)]
    pub accepted_content_types: Vec<String>,
    #[serde(default)]
    pub max_bytes: Option<u64>,
    #[serde(default = "default_true")]
    pub replayable: bool,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl ModuleNetworkAssetLane {
    pub fn normalize(mut self) -> Self {
        self.lane_id = canonical_asset_lane_id(&self.lane_id, &self.label, &self.artifact_kinds);
        self.label = self.label.trim().to_string();
        if self.label.is_empty() {
            self.label = self
                .lane_id
                .replace(['-', '_'], " ")
                .split_whitespace()
                .map(|part| {
                    let mut chars = part.chars();
                    match chars.next() {
                        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
        }
        self.artifact_kinds = normalize_vec(self.artifact_kinds);
        self.accepted_content_types = normalize_vec(self.accepted_content_types);
        self.notes = normalize_vec(self.notes);
        if matches!(self.max_bytes, Some(0)) {
            self.max_bytes = None;
        }
        self
    }

    pub fn merge(self, other: Self) -> Self {
        let mut merged = self;
        merged.label = if other.label.trim().is_empty() {
            merged.label
        } else {
            other.label.trim().to_string()
        };
        merged.direction = merged.direction.merge(other.direction);
        merged.delivery_mode = other.delivery_mode;
        merged.artifact_kinds.extend(other.artifact_kinds);
        merged
            .accepted_content_types
            .extend(other.accepted_content_types);
        merged.notes.extend(other.notes);
        merged.max_bytes = match (merged.max_bytes, other.max_bytes) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        };
        merged.replayable = merged.replayable && other.replayable;
        merged.normalize()
    }

    pub fn supports_receive(&self) -> bool {
        self.direction.supports_receive()
    }

    #[allow(dead_code)]
    pub fn supports_send(&self) -> bool {
        self.direction.supports_send()
    }

    pub fn matches_artifact(&self, kind: &str, content_type: &str, byte_len: u64) -> bool {
        if !self.supports_receive() {
            return false;
        }
        if self.max_bytes.is_some_and(|max_bytes| byte_len > max_bytes) {
            return false;
        }

        let kind = kind.trim().to_ascii_lowercase();
        let expected_kinds = if self.artifact_kinds.is_empty() {
            vec![self.lane_id.to_ascii_lowercase()]
        } else {
            self.artifact_kinds
                .iter()
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        };
        let kind_matches = expected_kinds.iter().any(|expected| {
            expected == "*"
                || (!kind.is_empty()
                    && (expected == &kind
                        || expected
                            .strip_suffix("/*")
                            .is_some_and(|prefix| kind.starts_with(prefix))))
        });
        if !kind_matches {
            return false;
        }

        if self.accepted_content_types.is_empty() {
            return true;
        }
        let content_type = content_type.trim().to_ascii_lowercase();
        self.accepted_content_types.iter().any(|accepted| {
            let accepted = accepted.to_ascii_lowercase();
            accepted == "*"
                || accepted == content_type
                || accepted
                    .strip_suffix("/*")
                    .is_some_and(|prefix| content_type.starts_with(prefix))
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleNetworkCapabilities {
    #[serde(default)]
    pub features: Vec<ModuleNetworkFeature>,
    #[serde(default)]
    pub asset_lanes: Vec<ModuleNetworkAssetLane>,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl ModuleNetworkCapabilities {
    pub fn normalize(mut self) -> Self {
        self.features.sort();
        self.features.dedup();
        self.asset_lanes = self
            .asset_lanes
            .into_iter()
            .map(ModuleNetworkAssetLane::normalize)
            .filter(|lane| !lane.lane_id.is_empty())
            .fold(Vec::<ModuleNetworkAssetLane>::new(), |mut lanes, lane| {
                if let Some(existing) = lanes
                    .iter_mut()
                    .find(|existing| existing.lane_id == lane.lane_id)
                {
                    *existing = existing.clone().merge(lane);
                } else {
                    lanes.push(lane);
                }
                lanes
            });
        self.notes = normalize_vec(self.notes);
        self
    }

    pub fn merge(self, other: Self) -> Self {
        let mut merged = self;
        merged.features.extend(other.features);
        merged.asset_lanes.extend(other.asset_lanes);
        merged.notes.extend(other.notes);
        merged.normalize()
    }

    pub fn is_empty(&self) -> bool {
        self.features.is_empty() && self.asset_lanes.is_empty() && self.notes.is_empty()
    }

    pub fn has(&self, feature: ModuleNetworkFeature) -> bool {
        self.features.contains(&feature)
    }

    pub fn matching_receive_asset_lanes<'a>(
        &'a self,
        kind: &str,
        content_type: &str,
        byte_len: u64,
    ) -> Vec<&'a ModuleNetworkAssetLane> {
        self.asset_lanes
            .iter()
            .filter(|lane| lane.matches_artifact(kind, content_type, byte_len))
            .collect()
    }
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
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default = "default_roles")]
    pub roles: Vec<String>,
    #[serde(default)]
    pub entry: Option<ModuleEntry>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default, skip_serializing)]
    pub visual_load: Option<ModuleVisualLoad>,
    #[serde(default)]
    pub network_capabilities: Option<ModuleNetworkCapabilities>,
}

#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub manifest: ModuleManifest,
    pub folder: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PortableManifest {
    #[serde(default)]
    module_id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    order: Option<i32>,
    #[serde(default)]
    visual_load: Option<ModuleVisualLoad>,
    #[serde(default)]
    network_capabilities: Option<ModuleNetworkCapabilities>,
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

        let manifest = if path.join("manifest.json").is_file() {
            match load_portable_manifest(&path) {
                Ok(Some(manifest)) => Some(manifest),
                Ok(None) => None,
                Err(err) => {
                    eprintln!(
                        "[modules] Invalid manifest.json in {:?}: {}",
                        path.file_name().unwrap_or_default(),
                        err
                    );
                    None
                }
            }
        } else if path.join("module.json").is_file() {
            match load_legacy_manifest(&path) {
                Ok(Some(manifest)) => Some(manifest),
                Ok(None) => None,
                Err(err) => {
                    eprintln!(
                        "[modules] Invalid module.json in {:?}: {}",
                        path.file_name().unwrap_or_default(),
                        err
                    );
                    None
                }
            }
        } else {
            eprintln!(
                "[modules] Skipping {:?} (no manifest.json or module.json found)",
                path.file_name().unwrap_or_default()
            );
            None
        };

        if let Some(manifest) = manifest {
            results.push(LoadedModule {
                manifest,
                folder: path,
            });
        }
    }

    results.sort_by(|a, b| {
        let a_order = a.manifest.order.unwrap_or(i32::MAX);
        let b_order = b.manifest.order.unwrap_or(i32::MAX);
        a_order.cmp(&b_order).then_with(|| {
            a.manifest
                .title
                .to_lowercase()
                .cmp(&b.manifest.title.to_lowercase())
        })
    });

    Ok(results)
}

fn load_legacy_manifest(folder: &Path) -> io::Result<Option<ModuleManifest>> {
    let manifest_path = folder.join("module.json");
    let manifest_str = fs::read_to_string(&manifest_path)?;
    let mut manifest: ModuleManifest = serde_json::from_str(&manifest_str)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    manifest.id = manifest.id.trim().to_string();
    manifest.title = manifest.title.trim().to_string();
    manifest.description = non_empty_opt(manifest.description.take());
    manifest.author = non_empty_opt(manifest.author.take());
    manifest.version = non_empty_opt(manifest.version.take());
    manifest.icon = non_empty_opt(manifest.icon.take());
    manifest.roles = normalize_roles(manifest.roles);
    manifest.permissions = normalize_vec(manifest.permissions);
    manifest.visual_load = load_visual_load(folder).ok().flatten();
    manifest.network_capabilities = merge_network_capabilities(
        manifest.network_capabilities.take(),
        load_network_capabilities(folder).ok().flatten(),
    );

    if manifest.id.is_empty() || manifest.title.is_empty() {
        return Ok(None);
    }

    Ok(Some(manifest))
}

fn load_portable_manifest(folder: &Path) -> io::Result<Option<ModuleManifest>> {
    let manifest_path = folder.join("manifest.json");
    let manifest_str = fs::read_to_string(&manifest_path)?;
    let portable: PortableManifest = serde_json::from_str(&manifest_str)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    let id = portable.module_id.trim().to_string();
    let title = portable.display_name.trim().to_string();
    if id.is_empty() || title.is_empty() {
        return Ok(None);
    }

    let visual_load = portable
        .visual_load
        .or_else(|| load_visual_load(folder).ok().flatten());

    Ok(Some(ModuleManifest {
        id,
        title,
        description: non_empty_string(portable.description),
        version: non_empty_opt(portable.version),
        author: non_empty_opt(portable.author),
        roles: normalize_roles(portable.roles),
        entry: discover_fallback_entry(folder),
        icon: non_empty_string(portable.icon),
        permissions: normalize_vec(portable.permissions),
        order: portable.order,
        visual_load,
        network_capabilities: merge_network_capabilities(
            portable.network_capabilities,
            load_network_capabilities(folder).ok().flatten(),
        ),
    }))
}

fn load_visual_load(folder: &Path) -> io::Result<Option<ModuleVisualLoad>> {
    let visual_path = folder.join("visual_load.json");
    if !visual_path.is_file() {
        return Ok(None);
    }
    let json = fs::read_to_string(&visual_path)?;
    let visual: ModuleVisualLoad = serde_json::from_str(&json)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    Ok(Some(visual))
}

fn load_network_capabilities(folder: &Path) -> io::Result<Option<ModuleNetworkCapabilities>> {
    let path = folder.join("network_capabilities.json");
    if !path.is_file() {
        return Ok(None);
    }
    let json = fs::read_to_string(&path)?;
    let caps: ModuleNetworkCapabilities = serde_json::from_str(&json)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    Ok(Some(caps.normalize()))
}

fn discover_fallback_entry(folder: &Path) -> Option<ModuleEntry> {
    for candidate in ["README.md", "HANDSHAKE.md", "STATE_TEMPLATE.md"] {
        if folder.join(candidate).is_file() {
            return Some(ModuleEntry::Markdown {
                path: candidate.to_string(),
            });
        }
    }

    for candidate in ["index.html", "web/index.html"] {
        if folder.join(candidate).is_file() {
            return Some(ModuleEntry::StaticHtml {
                path: candidate.to_string(),
            });
        }
    }

    None
}

fn normalize_roles(roles: Vec<String>) -> Vec<String> {
    let normalized = normalize_vec(roles);
    if normalized.is_empty() {
        default_roles()
    } else {
        normalized
    }
}

fn normalize_vec(values: Vec<String>) -> Vec<String> {
    let mut values = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn merge_network_capabilities(
    inline: Option<ModuleNetworkCapabilities>,
    file: Option<ModuleNetworkCapabilities>,
) -> Option<ModuleNetworkCapabilities> {
    match (inline, file) {
        (Some(inline), Some(file)) => Some(inline.merge(file)),
        (Some(inline), None) => Some(inline.normalize()),
        (None, Some(file)) => Some(file.normalize()),
        (None, None) => None,
    }
    .filter(|caps| !caps.is_empty())
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn non_empty_opt(value: Option<String>) -> Option<String> {
    value.and_then(non_empty_string)
}

fn default_true() -> bool {
    true
}

fn canonical_asset_lane_id(raw: &str, label: &str, kinds: &[String]) -> String {
    for candidate in [raw, label]
        .into_iter()
        .chain(kinds.first().map(|value| value.as_str()))
    {
        let normalized = candidate
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches('_')
            .to_string();
        if !normalized.is_empty() {
            return normalized;
        }
    }
    String::new()
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
            entry: Some(ModuleEntry::BuiltinPanel {
                target: "homework_dashboard".to_string(),
            }),
            icon: None,
            permissions: vec![],
            order: Some(-100),
            visual_load: None,
            network_capabilities: None,
        },
    )?;

    ensure_module(
        modules_root,
        "homework_assignments",
        ModuleManifest {
            id: "homework_assignments".to_string(),
            title: "Homework & Revision".to_string(),
            description: Some("View homework questions and revision tips".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Chatty-EDU".to_string()),
            roles: vec!["teacher".to_string(), "student".to_string()],
            entry: Some(ModuleEntry::BuiltinPanel {
                target: "homework_assignments".to_string(),
            }),
            icon: None,
            permissions: vec![],
            order: Some(-90),
            visual_load: None,
            network_capabilities: None,
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
