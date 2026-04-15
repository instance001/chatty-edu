use anyhow::{bail, Context, Result};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{
    http::{header::CONTENT_TYPE, Request, Response},
    WebViewBuilder,
};

#[allow(dead_code)]
#[path = "../module_bridge.rs"]
mod module_bridge;

use module_bridge::{
    append_bridge_outgoing_room_event, clear_bridge_shared_state, clear_bridge_status,
    read_bridge_incoming_assets, read_bridge_incoming_shared_state, read_bridge_shared_room_events,
    read_bridge_shared_room_state, remove_bridge_incoming_asset, write_bridge_shared_state,
    write_bridge_status, ModuleBridgeOutgoingRoomEvent, ModuleBridgeSharedRoomEvents,
    ModuleBridgeSharedRoomState, ModuleBridgeSharedState, ModuleBridgeStatus,
};

const MODULE_FILE_SCHEME: &str = "chattyedumodule";

fn main() -> Result<()> {
    let args = parse_args()?;
    let init_script = build_init_script(args.module_dir.as_deref());
    let bridge_dir = args.module_dir.clone();
    let hosted_content = resolve_hosted_content(&args)?;
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(args.title.clone())
        .with_inner_size(LogicalSize::new(1200.0, 800.0))
        .build(&event_loop)
        .context("failed to create webview host window")?;

    let mut builder = WebViewBuilder::new()
        .with_initialization_script(&init_script)
        .with_ipc_handler(move |req| {
            if let Some(module_dir) = bridge_dir.as_deref() {
                if let Err(err) = handle_ipc_message(module_dir, req.body().clone()) {
                    eprintln!("chatty-edu webview bridge error: {err:#}");
                }
            }
        });

    let target_url = match hosted_content {
        HostedContent::DirectUrl(url) => url,
        HostedContent::ModuleFiles {
            root_dir,
            entry_path,
        } => {
            let entry_path_for_url = entry_path.clone();
            builder = builder
                .with_custom_protocol(MODULE_FILE_SCHEME.into(), move |_id, request| {
                    serve_module_request(&root_dir, &entry_path_for_url, request)
                });
            format!("{MODULE_FILE_SCHEME}://localhost/{entry_path}")
        }
    };

    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    ))]
    let _webview = builder
        .with_url(&target_url)
        .build(&window)
        .context("failed to build embedded webview")?;

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    )))]
    let _webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window
            .default_vbox()
            .context("failed to create unix webview host container")?;
        builder
            .with_url(&target_url)
            .build_gtk(vbox)
            .context("failed to build unix embedded webview")?
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

struct Args {
    title: String,
    url: String,
    module_dir: Option<PathBuf>,
}

enum HostedContent {
    DirectUrl(String),
    ModuleFiles {
        root_dir: PathBuf,
        entry_path: String,
    },
}

fn parse_args() -> Result<Args> {
    let mut title = None;
    let mut url = None;
    let mut module_dir = None;

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--title" => title = iter.next(),
            "--url" => url = iter.next(),
            "--module-dir" => module_dir = iter.next().map(PathBuf::from),
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {}
        }
    }

    let title = title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Chatty-EDU Webview".to_string());
    let url = url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required --url"))?;

    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")) {
        bail!("--url must start with http://, https://, or file://");
    }

    Ok(Args {
        title,
        url,
        module_dir,
    })
}

fn print_help() {
    eprintln!(
        "chatty_edu_webview_host --title <window title> --url <http(s)://...|file://...> [--module-dir <path>]"
    );
}

fn resolve_hosted_content(args: &Args) -> Result<HostedContent> {
    if !args.url.starts_with("file://") {
        return Ok(HostedContent::DirectUrl(args.url.clone()));
    }

    let parsed = url::Url::parse(&args.url).context("parse file URL")?;
    let file_path = parsed
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("file URL could not be converted to a local path"))?;
    let root_dir = if let Some(module_dir) = args.module_dir.as_ref() {
        module_dir.to_path_buf()
    } else {
        file_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow::anyhow!("file URL has no parent directory"))?
    };

    let root_dir = root_dir
        .canonicalize()
        .with_context(|| format!("canonicalize {}", root_dir.display()))?;
    let file_path = file_path
        .canonicalize()
        .with_context(|| format!("canonicalize {}", file_path.display()))?;

    let rel = file_path
        .strip_prefix(&root_dir)
        .map_err(|_| anyhow::anyhow!("file URL target is outside the hosted module directory"))?;
    let entry_path = rel
        .iter()
        .map(|component| component.to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>()
        .join("/");

    if entry_path.trim().is_empty() {
        bail!("file URL target resolved to an empty module-relative path");
    }

    Ok(HostedContent::ModuleFiles {
        root_dir,
        entry_path,
    })
}

fn build_init_script(module_dir: Option<&Path>) -> String {
    let module_dir_js = serde_json::to_string(&module_dir.map(|path| path.display().to_string()))
        .unwrap_or_else(|_| "null".to_string());
    let bridge_enabled = if module_dir.is_some() {
        "true"
    } else {
        "false"
    };
    format!(
        r#"
(function() {{
  const bridge = {{
    available: {bridge_enabled},
    hosted: true,
    moduleDir: {module_dir_js},
    updateStatus(payload) {{
      if (!window.ipc || typeof window.ipc.postMessage !== "function") return false;
      try {{
        window.ipc.postMessage(JSON.stringify({{ kind: "bridge.update_status", payload: payload || {{}} }}));
        return true;
      }} catch (err) {{
        console.warn("chattyEduBridge.updateStatus failed", err);
        return false;
      }}
    }},
    clearStatus() {{
      if (!window.ipc || typeof window.ipc.postMessage !== "function") return false;
      try {{
        window.ipc.postMessage(JSON.stringify({{ kind: "bridge.clear_status" }}));
        return true;
      }} catch (err) {{
        console.warn("chattyEduBridge.clearStatus failed", err);
        return false;
      }}
    }},
    updateSharedState(payload) {{
      if (!window.ipc || typeof window.ipc.postMessage !== "function") return false;
      try {{
        window.ipc.postMessage(JSON.stringify({{ kind: "bridge.update_shared_state", payload: payload || {{}} }}));
        return true;
      }} catch (err) {{
        console.warn("chattyEduBridge.updateSharedState failed", err);
        return false;
      }}
    }},
    clearSharedState() {{
      if (!window.ipc || typeof window.ipc.postMessage !== "function") return false;
      try {{
        window.ipc.postMessage(JSON.stringify({{ kind: "bridge.clear_shared_state" }}));
        return true;
      }} catch (err) {{
        console.warn("chattyEduBridge.clearSharedState failed", err);
        return false;
      }}
    }},
    async readIncomingSharedState() {{
      if (!bridge.available) return null;
      try {{
        const response = await fetch("/__chattyedu_bridge__/incoming_shared_state.json?ts=" + Date.now(), {{ cache: "no-store" }});
        if (!response.ok) return null;
        return await response.json();
      }} catch (err) {{
        console.warn("chattyEduBridge.readIncomingSharedState failed", err);
        return null;
      }}
    }},
    async readSharedRoomState() {{
      if (!bridge.available) return null;
      try {{
        const response = await fetch("/__chattyedu_bridge__/shared_room_state.json?ts=" + Date.now(), {{ cache: "no-store" }});
        if (!response.ok) return null;
        return await response.json();
      }} catch (err) {{
        console.warn("chattyEduBridge.readSharedRoomState failed", err);
        return null;
      }}
    }},
    async readSharedRoomEvents() {{
      if (!bridge.available) return null;
      try {{
        const response = await fetch("/__chattyedu_bridge__/shared_room_events.json?ts=" + Date.now(), {{ cache: "no-store" }});
        if (!response.ok) return null;
        return await response.json();
      }} catch (err) {{
        console.warn("chattyEduBridge.readSharedRoomEvents failed", err);
        return null;
      }}
    }},
    emitRoomEvent(payload) {{
      if (!window.ipc || typeof window.ipc.postMessage !== "function") return false;
      try {{
        window.ipc.postMessage(JSON.stringify({{ kind: "bridge.emit_room_event", payload: payload || {{}} }}));
        return true;
      }} catch (err) {{
        console.warn("chattyEduBridge.emitRoomEvent failed", err);
        return false;
      }}
    }},
    async readIncomingAssets(laneId) {{
      if (!bridge.available || !laneId) return [];
      try {{
        const response = await fetch("/__chattyedu_bridge__/incoming_assets.json?lane=" + encodeURIComponent(laneId) + "&ts=" + Date.now(), {{ cache: "no-store" }});
        if (!response.ok) return [];
        const payload = await response.json();
        return Array.isArray(payload) ? payload : [];
      }} catch (err) {{
        console.warn("chattyEduBridge.readIncomingAssets failed", err);
        return [];
      }}
    }},
    incomingAssetUrl(laneId, payloadFileName) {{
      if (!bridge.available || !laneId || !payloadFileName) return "";
      return "/__chattyedu_bridge__/incoming_assets/" + encodeURIComponent(laneId) + "/" + encodeURIComponent(payloadFileName);
    }},
    consumeIncomingAsset(laneId, assetId) {{
      if (!window.ipc || typeof window.ipc.postMessage !== "function" || !laneId || !assetId) return false;
      try {{
        window.ipc.postMessage(JSON.stringify({{
          kind: "bridge.consume_incoming_asset",
          payload: {{ lane_id: laneId, asset_id: assetId }}
        }}));
        return true;
      }} catch (err) {{
        console.warn("chattyEduBridge.consumeIncomingAsset failed", err);
        return false;
      }}
    }}
  }};
  window.chattyEduBridge = Object.assign({{}}, window.chattyEduBridge || {{}}, bridge);
}})();
"#
    )
}

#[derive(serde::Deserialize)]
struct IpcEnvelope {
    kind: String,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(serde::Deserialize)]
struct ConsumeIncomingAssetPayload {
    lane_id: String,
    asset_id: String,
}

fn handle_ipc_message(module_dir: &Path, msg: String) -> Result<()> {
    let envelope: IpcEnvelope = serde_json::from_str(&msg).context("parse IPC envelope")?;
    match envelope.kind.as_str() {
        "bridge.update_status" => {
            let mut status: ModuleBridgeStatus =
                serde_json::from_value(envelope.payload).context("parse bridge status payload")?;
            status.normalize();
            write_bridge_status(module_dir, &status)
        }
        "bridge.clear_status" => clear_bridge_status(module_dir),
        "bridge.update_shared_state" => {
            let mut state: ModuleBridgeSharedState = serde_json::from_value(envelope.payload)
                .context("parse bridge shared-state payload")?;
            state.normalize();
            write_bridge_shared_state(module_dir, &state)
        }
        "bridge.clear_shared_state" => clear_bridge_shared_state(module_dir),
        "bridge.emit_room_event" => {
            let mut event: ModuleBridgeOutgoingRoomEvent = serde_json::from_value(envelope.payload)
                .context("parse bridge room-event payload")?;
            event.normalize();
            append_bridge_outgoing_room_event(module_dir, &event)
        }
        "bridge.consume_incoming_asset" => {
            let payload: ConsumeIncomingAssetPayload = serde_json::from_value(envelope.payload)
                .context("parse consume incoming-asset payload")?;
            remove_bridge_incoming_asset(module_dir, &payload.lane_id, &payload.asset_id)
                .map(|_| ())
        }
        other => bail!("unknown IPC kind: {other}"),
    }
}

fn serve_module_request(
    root_dir: &Path,
    entry_path: &str,
    request: Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    match try_serve_module_request(root_dir, entry_path, request) {
        Ok(response) => response.map(Cow::Owned),
        Err(err) => Response::builder()
            .status(500)
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Cow::Owned(err.to_string().into_bytes()))
            .unwrap(),
    }
}

fn try_serve_module_request(
    root_dir: &Path,
    entry_path: &str,
    request: Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>> {
    let request_path = normalize_request_path(request.uri().path(), entry_path);
    if request_path == PathBuf::from("__chattyedu_bridge__/incoming_shared_state.json") {
        let incoming = read_bridge_incoming_shared_state(root_dir)?;
        let body = serde_json::to_vec(&incoming).context("serialize incoming shared state")?;
        return Ok(Response::builder()
            .status(200)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(body)?);
    }
    if request_path == PathBuf::from("__chattyedu_bridge__/shared_room_state.json") {
        let room_state: Option<ModuleBridgeSharedRoomState> =
            read_bridge_shared_room_state(root_dir)?;
        let body = serde_json::to_vec(&room_state).context("serialize shared room state")?;
        return Ok(Response::builder()
            .status(200)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(body)?);
    }
    if request_path == PathBuf::from("__chattyedu_bridge__/shared_room_events.json") {
        let room_events: Option<ModuleBridgeSharedRoomEvents> =
            read_bridge_shared_room_events(root_dir)?;
        let body = serde_json::to_vec(&room_events).context("serialize shared room events")?;
        return Ok(Response::builder()
            .status(200)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(body)?);
    }
    if request_path == PathBuf::from("__chattyedu_bridge__/incoming_assets.json") {
        let lane_id = request
            .uri()
            .query()
            .and_then(parse_lane_query)
            .unwrap_or_default();
        let assets = if lane_id.is_empty() {
            Vec::new()
        } else {
            read_bridge_incoming_assets(root_dir, Some(&lane_id))?
        };
        let body = serde_json::to_vec(&assets).context("serialize incoming assets")?;
        return Ok(Response::builder()
            .status(200)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(body)?);
    }
    if request_path
        .iter()
        .map(|segment| segment.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .starts_with(&[
            "__chattyedu_bridge__".to_string(),
            "incoming_assets".to_string(),
        ])
    {
        let parts = request_path
            .iter()
            .map(|segment| segment.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        if parts.len() >= 4 {
            let lane_id = &parts[2];
            let payload_file_name = &parts[3];
            let assets = read_bridge_incoming_assets(root_dir, Some(lane_id))?;
            if let Some(asset) = assets
                .into_iter()
                .find(|asset| asset.payload_file_name == *payload_file_name)
            {
                let payload_path = module_bridge::bridge_incoming_asset_lane_dir(root_dir, lane_id)
                    .join(payload_file_name);
                if payload_path.is_file() {
                    let body = std::fs::read(&payload_path)
                        .with_context(|| format!("read {}", payload_path.display()))?;
                    return Ok(Response::builder()
                        .status(200)
                        .header(
                            CONTENT_TYPE,
                            if asset.content_type.trim().is_empty() {
                                mime_for_path(&payload_path)
                            } else {
                                &asset.content_type
                            },
                        )
                        .body(body)?);
                }
            }
        }
        return Ok(Response::builder()
            .status(404)
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(b"Not Found".to_vec())?);
    }
    let candidate = root_dir.join(&request_path);
    let root = root_dir
        .canonicalize()
        .with_context(|| format!("canonicalize {}", root_dir.display()))?;
    let resolved = candidate
        .canonicalize()
        .with_context(|| format!("canonicalize {}", candidate.display()))?;
    if !resolved.starts_with(&root) {
        return Ok(Response::builder()
            .status(403)
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(b"Forbidden".to_vec())?);
    }
    if !resolved.is_file() {
        return Ok(Response::builder()
            .status(404)
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(b"Not Found".to_vec())?);
    }

    let bytes = std::fs::read(&resolved).with_context(|| format!("read {}", resolved.display()))?;
    let mime = mime_for_path(&resolved);
    Ok(Response::builder()
        .status(200)
        .header(CONTENT_TYPE, mime)
        .body(bytes)?)
}

fn parse_lane_query(query: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == "lane" {
            return Some(percent_decode(value));
        }
    }
    None
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                if let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3]) {
                    if let Ok(decoded) = u8::from_str_radix(hex, 16) {
                        out.push(decoded);
                        index += 3;
                        continue;
                    }
                }
                out.push(bytes[index]);
                index += 1;
            }
            other => {
                out.push(other);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn normalize_request_path(request_path: &str, entry_path: &str) -> PathBuf {
    let mut raw = request_path.trim().trim_start_matches('/');
    if raw.is_empty() {
        raw = entry_path.trim_start_matches('/');
    }

    let mut out = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            std::path::Component::Normal(part) => out.push(part),
            _ => {}
        }
    }
    out
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "txt" | "log" | "md" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}
