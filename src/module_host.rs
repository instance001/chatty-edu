use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use module_host_support::{
    resolve_webview_host_binary, resolve_webview_target, run_command_to_completion,
    ModuleCommandSpec, ResolvedCommand, WebviewHostConfig,
};
use serde::Deserialize;

use crate::module_bridge::{bridge_env, clear_bridge_status};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModuleVisualLoad {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub auto_launch: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub launch: Option<ModuleCommandSpec>,
    #[serde(default)]
    pub build: Option<ModuleCommandSpec>,
    #[serde(default)]
    pub serve: Option<ModuleCommandSpec>,
    #[serde(default)]
    pub serve_wait_ms: Option<u64>,
    #[serde(default)]
    pub window_title_contains: Option<String>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HostRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub struct ModuleHostState {
    pub status: String,
    pub launch_attempted: bool,
    pub user_stopped: bool,
    pub close_requested: bool,
    child: Option<Child>,
    support_children: Vec<Child>,
    launched_at: Option<Instant>,
    attached_hwnd: Option<isize>,
    build_rx: Option<Receiver<String>>,
}

impl Default for ModuleHostState {
    fn default() -> Self {
        Self {
            status: "Ready.".to_string(),
            launch_attempted: false,
            user_stopped: false,
            close_requested: false,
            child: None,
            support_children: Vec::new(),
            launched_at: None,
            attached_hwnd: None,
            build_rx: None,
        }
    }
}

impl ModuleVisualLoad {
    pub fn hosts_native_window(&self) -> bool {
        matches!(
            self.kind.trim().to_ascii_lowercase().as_str(),
            "native_window" | "native-window" | "window" | "native" | "webview"
        )
    }

    pub fn is_webview(&self) -> bool {
        self.kind.trim().eq_ignore_ascii_case("webview")
    }

    fn window_hint(&self) -> Option<String> {
        self.window_title_contains
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                self.title
                    .as_ref()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
    }
}

impl ModuleHostState {
    pub fn is_running(&mut self) -> bool {
        self.poll_background();
        self.poll_process_exit();
        self.child.is_some()
    }

    pub fn is_waiting_for_window(&self) -> bool {
        self.child.is_some() && self.attached_hwnd.is_none()
    }

    pub fn start_build(
        &mut self,
        module_dir: &Path,
        spec: &ModuleVisualLoad,
    ) -> Result<(), String> {
        let Some(build) = spec.build.as_ref() else {
            return Err("This module does not advertise a build command.".to_string());
        };
        if self.build_rx.is_some() {
            return Err("Build already running.".to_string());
        }

        let resolved = ResolvedCommand::from_spec(module_dir, build)?;
        let (tx, rx) = crossbeam_channel::bounded(1);
        let summary = resolved.display_string();
        std::thread::spawn(move || {
            let result = run_command_to_completion(&resolved);
            let _ = tx.send(match result {
                Ok(msg) => format!("Build finished: {msg}"),
                Err(err) => format!("Build failed: {err}"),
            });
        });
        self.build_rx = Some(rx);
        self.status = format!("Building module UI via {summary}...");
        Ok(())
    }

    pub fn launch(&mut self, module_dir: &Path, spec: &ModuleVisualLoad) -> Result<(), String> {
        self.poll_background();
        self.poll_process_exit();
        if self.child.is_some() {
            return Err("Module UI is already running.".to_string());
        }

        if let Err(err) = clear_bridge_status(module_dir) {
            self.status = format!("Bridge reset warning: {err}");
        }

        if spec.is_webview() {
            return self.launch_webview(module_dir, spec);
        }

        let Some(launch) = spec.launch.as_ref() else {
            return Err("This module does not advertise a launch command.".to_string());
        };

        let resolved = ResolvedCommand::from_spec(module_dir, launch)?;
        if resolved.is_path_like && !resolved.program.exists() {
            return Err(format!(
                "Launch target not found: {}",
                resolved.program.display()
            ));
        }

        let mut command = Command::new(&resolved.program);
        command.args(&resolved.args);
        if let Some(cwd) = &resolved.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &resolved.env {
            command.env(key, value);
        }
        if let Ok(extra_env) = bridge_env(module_dir) {
            for (key, value) in extra_env {
                command.env(key, value);
            }
        }

        let child = command
            .spawn()
            .map_err(|err| format!("Failed to launch {}: {err}", resolved.display_string()))?;
        self.child = Some(child);
        self.launched_at = Some(Instant::now());
        self.attached_hwnd = None;
        self.launch_attempted = true;
        self.user_stopped = false;
        self.close_requested = false;
        self.status = "Launching native module UI...".to_string();
        Ok(())
    }

    fn launch_webview(&mut self, module_dir: &Path, spec: &ModuleVisualLoad) -> Result<(), String> {
        let helper = resolve_webview_host_binary(&WEBVIEW_HOST_CONFIG)?;
        if !helper.is_file() {
            return Err(format!(
                "Webview host binary not found: {}. Rebuild Chatty-EDU so the helper binary exists.",
                helper.display()
            ));
        }

        if let Some(serve) = spec.serve.as_ref() {
            let resolved = ResolvedCommand::from_spec(module_dir, serve)?;
            if resolved.is_path_like && !resolved.program.exists() {
                return Err(format!(
                    "Webview serve target not found: {}",
                    resolved.program.display()
                ));
            }

            let mut command = Command::new(&resolved.program);
            command.args(&resolved.args);
            if let Some(cwd) = &resolved.cwd {
                command.current_dir(cwd);
            }
            for (key, value) in &resolved.env {
                command.env(key, value);
            }
            if let Ok(extra_env) = bridge_env(module_dir) {
                for (key, value) in extra_env {
                    command.env(key, value);
                }
            }
            let child = command.spawn().map_err(|err| {
                format!(
                    "Failed to start webview server {}: {err}",
                    resolved.display_string()
                )
            })?;
            self.support_children.push(child);
        }

        if let Some(wait_ms) = spec.serve_wait_ms {
            if wait_ms > 0 {
                std::thread::sleep(Duration::from_millis(wait_ms));
            }
        } else if spec.serve.is_some() {
            std::thread::sleep(Duration::from_millis(1200));
        }

        let target_url =
            resolve_webview_target(module_dir, spec.url.as_deref(), spec.file.as_deref())?;
        let title = spec
            .title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Chatty-EDU Module".to_string());

        let mut command = Command::new(&helper);
        command.arg("--title").arg(&title);
        command.arg("--url").arg(&target_url);
        command.arg("--module-dir").arg(module_dir);

        let child = command
            .spawn()
            .map_err(|err| format!("Failed to launch webview host {}: {err}", helper.display()))?;

        self.child = Some(child);
        self.launched_at = Some(Instant::now());
        self.attached_hwnd = None;
        self.launch_attempted = true;
        self.user_stopped = false;
        self.close_requested = false;
        self.status = format!("Launching hosted webview for {title}...");
        Ok(())
    }

    pub fn request_close(&mut self, spec: &ModuleVisualLoad) {
        self.poll_background();
        self.poll_process_exit();
        if self.child.is_none() {
            self.close_requested = false;
            return;
        }

        let hwnd = self.attached_hwnd.or_else(|| {
            let pid = self.child.as_ref().map(|child| child.id())?;
            let hint = spec.window_hint();
            find_process_window(pid, hint.as_deref())
        });

        if let Some(hwnd) = hwnd {
            post_window_close(hwnd);
            self.close_requested = true;
            self.status = "Closing module UI...".to_string();
        } else {
            self.force_stop();
        }
    }

    pub fn force_stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stop_support_children();
        self.attached_hwnd = None;
        self.launched_at = None;
        self.close_requested = false;
        self.user_stopped = true;
        self.status = "Module UI stopped.".to_string();
    }

    pub fn sync(
        &mut self,
        module_dir: &Path,
        spec: &ModuleVisualLoad,
        target: Option<HostRect>,
    ) -> bool {
        self.poll_background();
        self.poll_process_exit();

        let mut needs_repaint = false;

        if target.is_some()
            && spec.auto_launch
            && !self.launch_attempted
            && !self.user_stopped
            && self.child.is_none()
            && self.build_rx.is_none()
        {
            if let Err(err) = self.launch(module_dir, spec) {
                self.status = err;
            }
            needs_repaint = true;
        }

        let Some(target) = target else {
            if let Some(hwnd) = self.attached_hwnd {
                show_window_hidden(hwnd);
            }
            return needs_repaint;
        };

        let Some(child) = self.child.as_ref() else {
            return needs_repaint;
        };

        let pid = child.id();
        if self.attached_hwnd.is_none() {
            let hint = spec.window_hint();
            self.attached_hwnd = find_process_window(pid, hint.as_deref());
        }

        if let Some(hwnd) = self.attached_hwnd {
            match attach_window(hwnd, target) {
                Ok(()) => {
                    self.status = if spec.is_webview() {
                        "Hosted webview live.".to_string()
                    } else {
                        "Hosted native UI live.".to_string()
                    };
                }
                Err(err) => {
                    self.status = format!("Host attach failed: {err}");
                }
            }
            needs_repaint = true;
        } else {
            let waited = self.launched_at.map(|t| t.elapsed()).unwrap_or_default();
            self.status = if waited > Duration::from_secs(10) {
                if spec.is_webview() {
                    "Webview host started, but no window was discovered.".to_string()
                } else {
                    "Module process started, but no native window was discovered.".to_string()
                }
            } else {
                "Waiting for module window...".to_string()
            };
            needs_repaint = true;
        }

        needs_repaint
    }

    pub fn ready_to_finish_close(&mut self) -> bool {
        self.poll_background();
        self.poll_process_exit();
        self.close_requested && self.child.is_none()
    }

    fn poll_background(&mut self) {
        let Some(rx) = &self.build_rx else {
            return;
        };
        if let Ok(message) = rx.try_recv() {
            self.status = message;
            self.build_rx = None;
        }
    }

    fn poll_process_exit(&mut self) {
        let Some(child) = self.child.as_mut() else {
            self.attached_hwnd = None;
            return;
        };

        match child.try_wait() {
            Ok(Some(status)) => {
                self.child = None;
                self.stop_support_children();
                self.attached_hwnd = None;
                self.launched_at = None;
                self.status = if self.close_requested {
                    "Module UI closed.".to_string()
                } else if let Some(code) = status.code() {
                    format!("Module UI exited (code {code}).")
                } else {
                    "Module UI exited.".to_string()
                };
            }
            Ok(None) => {}
            Err(err) => {
                self.child = None;
                self.stop_support_children();
                self.attached_hwnd = None;
                self.launched_at = None;
                self.status = format!("Module UI status check failed: {err}");
            }
        }
    }

    fn stop_support_children(&mut self) {
        for mut child in self.support_children.drain(..) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

const WEBVIEW_HOST_CONFIG: WebviewHostConfig<'static> = WebviewHostConfig {
    windows_binary_name: "chatty_edu_webview_host.exe",
    other_binary_name: "chatty_edu_webview_host",
    cargo_bin_name: "chatty_edu_webview_host",
};

#[cfg(target_os = "windows")]
fn find_process_window(pid: u32, title_hint: Option<&str>) -> Option<isize> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, GW_OWNER,
    };

    struct SearchState {
        pid: u32,
        title_hint: Option<String>,
        found: Option<HWND>,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
        let state = unsafe { &mut *(lparam as *mut SearchState) };
        let mut window_pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut window_pid);
        }
        if window_pid != state.pid {
            return 1;
        }
        if unsafe { IsWindowVisible(hwnd) } == 0 {
            return 1;
        }
        if unsafe { GetWindow(hwnd, GW_OWNER) } != std::ptr::null_mut() {
            return 1;
        }

        if let Some(title_hint) = &state.title_hint {
            let len = unsafe { GetWindowTextLengthW(hwnd) };
            if len <= 0 {
                return 1;
            }
            let mut buffer = vec![0u16; len as usize + 1];
            let written = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
            if written <= 0 {
                return 1;
            }
            let title = String::from_utf16_lossy(&buffer[..written as usize]).to_lowercase();
            if !title.contains(&title_hint.to_lowercase()) {
                return 1;
            }
        }

        state.found = Some(hwnd);
        0
    }

    let mut state = SearchState {
        pid,
        title_hint: title_hint
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        found: None,
    };

    unsafe {
        EnumWindows(Some(enum_proc), &mut state as *mut SearchState as LPARAM);
    }
    state.found.map(|hwnd| hwnd as isize)
}

#[cfg(not(target_os = "windows"))]
fn find_process_window(_pid: u32, _title_hint: Option<&str>) -> Option<isize> {
    None
}

#[cfg(target_os = "windows")]
fn attach_window(hwnd: isize, target: HostRect) -> Result<(), String> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SW_SHOW, WS_CAPTION,
        WS_CHILD, WS_EX_APPWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
        WS_THICKFRAME, WS_VISIBLE,
    };

    let parent = find_chatty_edu_window()
        .ok_or_else(|| "Chatty-EDU parent window not found.".to_string())?;
    unsafe {
        SetParent(hwnd as HWND, parent as HWND);
        let mut style =
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd as HWND, GWL_STYLE)
                as usize;
        style &= !(WS_POPUP as usize
            | WS_CAPTION as usize
            | WS_THICKFRAME as usize
            | WS_MINIMIZEBOX as usize
            | WS_MAXIMIZEBOX as usize
            | WS_SYSMENU as usize);
        style |= WS_CHILD as usize | WS_VISIBLE as usize;
        SetWindowLongPtrW(hwnd as HWND, GWL_STYLE, style as isize);

        let mut ex_style = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
            hwnd as HWND,
            GWL_EXSTYLE,
        ) as usize;
        ex_style &= !(WS_EX_APPWINDOW as usize);
        SetWindowLongPtrW(hwnd as HWND, GWL_EXSTYLE, ex_style as isize);

        ShowWindow(hwnd as HWND, SW_SHOW);
        SetWindowPos(
            hwnd as HWND,
            std::ptr::null_mut(),
            target.x,
            target.y,
            target.width.max(32),
            target.height.max(32),
            SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn attach_window(_hwnd: isize, _target: HostRect) -> Result<(), String> {
    Err("Native in-tab window hosting is currently Windows-only.".to_string())
}

#[cfg(target_os = "windows")]
fn post_window_close(hwnd: isize) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    unsafe {
        PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
    }
}

#[cfg(not(target_os = "windows"))]
fn post_window_close(_hwnd: isize) {}

#[cfg(target_os = "windows")]
fn show_window_hidden(hwnd: isize) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

    unsafe {
        ShowWindow(hwnd as HWND, SW_HIDE);
    }
}

#[cfg(not(target_os = "windows"))]
fn show_window_hidden(_hwnd: isize) {}

#[cfg(target_os = "windows")]
fn find_chatty_edu_window() -> Option<isize> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    let title: Vec<u16> = "Chatty-EDU"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let hwnd: HWND = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    (hwnd != std::ptr::null_mut()).then_some(hwnd as isize)
}

#[cfg(not(target_os = "windows"))]
fn find_chatty_edu_window() -> Option<isize> {
    None
}
