use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct EcgWindowPayload {
    pub supported: bool,
    pub available: bool,
    pub label: String,
    pub note: String,
    pub current_percent: f32,
    pub history: Vec<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct EcgPoint {
    pub x: f32,
    pub y: f32,
}

pub struct EcgWindowState {
    payload: EcgWindowPayload,
    max_history: usize,
    sample_interval: Duration,
    last_sample_at: Instant,
    backend: EcgBackend,
}

impl EcgWindowState {
    pub fn new(label: impl Into<String>) -> Self {
        let max_history = 48;
        let sample_interval = Duration::from_millis(1500);
        let mut payload = EcgWindowPayload {
            supported: true,
            available: false,
            label: label.into(),
            note: String::new(),
            current_percent: 0.0,
            history: vec![0.0; max_history],
        };
        let backend = EcgBackend::new(&mut payload);
        Self {
            payload,
            max_history,
            sample_interval,
            last_sample_at: Instant::now(),
            backend,
        }
    }

    pub fn refresh_interval(&self) -> Duration {
        self.sample_interval
    }

    pub fn tick(&mut self, now: Instant) {
        if now.duration_since(self.last_sample_at) >= self.sample_interval {
            self.last_sample_at = now;
            self.sample_once();
        }
    }

    pub fn record_activity(&mut self, intensity: f32, note: impl Into<String>) {
        if let EcgBackend::Pulse(pulse) = &mut self.backend {
            pulse.record_activity(intensity, note.into());
        }
    }

    pub fn payload(&self) -> &EcgWindowPayload {
        &self.payload
    }

    pub fn points(&self, width: f32, height: f32) -> Vec<EcgPoint> {
        build_points(&self.payload.history, width, height)
    }

    fn sample_once(&mut self) {
        match &mut self.backend {
            EcgBackend::Pulse(pulse) => pulse.sample_once(&mut self.payload, self.max_history),
            #[cfg(windows)]
            EcgBackend::System(system) => system.sample_once(&mut self.payload, self.max_history),
        }
    }
}

enum EcgBackend {
    #[cfg(windows)]
    System(SystemTelemetry),
    Pulse(PulseTelemetry),
}

impl EcgBackend {
    fn new(payload: &mut EcgWindowPayload) -> Self {
        #[cfg(windows)]
        if let Ok(system) = SystemTelemetry::new(payload) {
            return Self::System(system);
        }

        #[cfg(not(windows))]
        {
            payload.note =
                "System counters are not available on this OS. Monitoring Chatty-EDU activity."
                    .to_string();
        }

        #[cfg(windows)]
        if payload.note.is_empty() {
            payload.note =
                "System counters were unavailable. Monitoring Chatty-EDU activity instead."
                    .to_string();
        }

        payload.available = true;
        Self::Pulse(PulseTelemetry::default())
    }
}

#[derive(Default)]
struct PulseTelemetry {
    current_signal: f32,
    active_note: Option<String>,
    note_hold_samples: usize,
}

impl PulseTelemetry {
    fn record_activity(&mut self, intensity: f32, note: String) {
        self.current_signal = self.current_signal.max(intensity.clamp(0.0, 100.0));
        self.active_note = Some(note);
        self.note_hold_samples = 4;
    }

    fn sample_once(&mut self, payload: &mut EcgWindowPayload, max_history: usize) {
        let value = self.current_signal.clamp(0.0, 100.0);
        payload.available = true;
        payload.current_percent = value;
        push_history(payload, max_history, value);

        if value >= 1.0 {
            if self.note_hold_samples > 0 {
                if let Some(note) = self.active_note.as_ref() {
                    payload.note = note.clone();
                }
                self.note_hold_samples = self.note_hold_samples.saturating_sub(1);
            } else if value >= 65.0 {
                payload.note = "Recent Chatty-EDU workload activity is high.".to_string();
            } else {
                payload.note = "Recent Chatty-EDU workload activity is steady.".to_string();
            }
        } else {
            payload.note = "Chatty-EDU is idle right now.".to_string();
            self.active_note = None;
            self.note_hold_samples = 0;
        }

        self.current_signal *= 0.74;
        if self.current_signal < 1.5 {
            self.current_signal = 0.0;
        }
    }
}

fn push_history(payload: &mut EcgWindowPayload, max_history: usize, value: f32) {
    payload.history.push(value.clamp(0.0, 100.0));
    if payload.history.len() > max_history {
        let overflow = payload.history.len() - max_history;
        payload.history.drain(0..overflow);
    }
}

#[cfg(windows)]
mod windows_backend {
    use super::{push_history, EcgWindowPayload};
    use std::iter;
    use windows_sys::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhGetFormattedCounterValue, PdhOpenQueryW, PDH_CSTATUS_INVALID_DATA,
        PDH_CSTATUS_ITEM_NOT_VALIDATED, PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_NO_INSTANCE,
        PDH_CSTATUS_NO_MACHINE, PDH_CSTATUS_NO_OBJECT, PDH_CSTATUS_VALID_DATA,
        PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER,
        PDH_HQUERY, PDH_INVALID_DATA, PDH_MORE_DATA, PDH_NO_DATA,
    };

    const STATUS_SUCCESS: u32 = 0;
    const GPU_COUNTER_PATH: &str = r"\GPU Engine(*)\Utilization Percentage";
    const CPU_UTILITY_PATH: &str = r"\Processor Information(_Total)\% Processor Utility";
    const CPU_TIME_PATH: &str = r"\Processor(_Total)\% Processor Time";

    pub(super) struct SystemTelemetry {
        query: PDH_HQUERY,
        gpu_counter: Option<PDH_HCOUNTER>,
        cpu_counter: Option<PDH_HCOUNTER>,
        cpu_note: &'static str,
    }

    impl SystemTelemetry {
        pub(super) fn new(payload: &mut EcgWindowPayload) -> Result<Self, String> {
            let mut query: PDH_HQUERY = std::ptr::null_mut();
            let status = unsafe {
                // Open a PDH query so we can sample Task Manager-style performance counters.
                PdhOpenQueryW(std::ptr::null(), 0, &mut query)
            };
            if status != STATUS_SUCCESS {
                return Err(format_pdh_error("open performance query", status));
            }

            let gpu_counter = add_counter(query, GPU_COUNTER_PATH).ok();
            let (cpu_counter, cpu_note) = if let Ok(counter) = add_counter(query, CPU_UTILITY_PATH)
            {
                (Some(counter), "Task Manager-style total CPU utility.")
            } else if let Ok(counter) = add_counter(query, CPU_TIME_PATH) {
                (Some(counter), "Task Manager-style total CPU usage.")
            } else {
                (None, "")
            };

            if gpu_counter.is_none() && cpu_counter.is_none() {
                unsafe {
                    let _ = PdhCloseQuery(query);
                }
                return Err("No usable GPU or CPU performance counters were found.".to_string());
            }

            let status = unsafe {
                // Prime the query so the next scheduled sample has real deltas to read.
                PdhCollectQueryData(query)
            };
            if status != STATUS_SUCCESS {
                unsafe {
                    let _ = PdhCloseQuery(query);
                }
                return Err(format_pdh_error("prime performance query", status));
            }

            payload.supported = true;
            payload.available = false;
            payload.note = if gpu_counter.is_some() && cpu_counter.is_some() {
                "Monitoring system GPU and CPU activity. Showing the busiest detected subsystem."
                    .to_string()
            } else if gpu_counter.is_some() {
                "Monitoring system GPU activity from Windows performance counters.".to_string()
            } else {
                "Monitoring system CPU activity from Windows performance counters.".to_string()
            };

            Ok(Self {
                query,
                gpu_counter,
                cpu_counter,
                cpu_note,
            })
        }

        pub(super) fn sample_once(&mut self, payload: &mut EcgWindowPayload, max_history: usize) {
            let collect_status = unsafe { PdhCollectQueryData(self.query) };
            if collect_status != STATUS_SUCCESS {
                payload.available = false;
                payload.current_percent = 0.0;
                payload.note = format_pdh_error("refresh system counters", collect_status);
                push_history(payload, max_history, 0.0);
                return;
            }

            let mut issues = Vec::new();
            let gpu_percent = if let Some(counter) = self.gpu_counter {
                match read_gpu_percent(counter) {
                    Ok(value) => value,
                    Err(err) => {
                        issues.push(err);
                        None
                    }
                }
            } else {
                None
            };

            let cpu_percent = if let Some(counter) = self.cpu_counter {
                match read_single_counter_percent(counter) {
                    Ok(value) => value,
                    Err(err) => {
                        issues.push(err);
                        None
                    }
                }
            } else {
                None
            };

            let (value, note, available) = match (gpu_percent, cpu_percent) {
                (Some(gpu), Some(cpu)) => {
                    let shown = gpu.max(cpu);
                    (
                        shown,
                        format!(
                            "System activity from the busiest detected subsystem. GPU {:.0}% | CPU {:.0}%.",
                            gpu, cpu
                        ),
                        true,
                    )
                }
                (Some(gpu), None) => (
                    gpu,
                    "Task Manager-style GPU engine utilization.".to_string(),
                    true,
                ),
                (None, Some(cpu)) => (cpu, self.cpu_note.to_string(), true),
                (None, None) => {
                    let note = if issues.is_empty() {
                        "Waiting for Windows performance counters to return data.".to_string()
                    } else {
                        issues.join(" | ")
                    };
                    (0.0, note, false)
                }
            };

            payload.available = available;
            payload.current_percent = value.clamp(0.0, 100.0);
            payload.note = note;
            push_history(payload, max_history, payload.current_percent);
        }
    }

    impl Drop for SystemTelemetry {
        fn drop(&mut self) {
            if !self.query.is_null() {
                unsafe {
                    let _ = PdhCloseQuery(self.query);
                }
            }
        }
    }

    fn add_counter(query: PDH_HQUERY, path: &str) -> Result<PDH_HCOUNTER, String> {
        let wide = wide_string(path);
        let mut counter: PDH_HCOUNTER = std::ptr::null_mut();
        let status = unsafe { PdhAddEnglishCounterW(query, wide.as_ptr(), 0, &mut counter) };
        if status == STATUS_SUCCESS {
            Ok(counter)
        } else {
            Err(format_pdh_error(&format!("add counter {path}"), status))
        }
    }

    fn read_single_counter_percent(counter: PDH_HCOUNTER) -> Result<Option<f32>, String> {
        let mut value = PDH_FMT_COUNTERVALUE::default();
        let status = unsafe {
            PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, std::ptr::null_mut(), &mut value)
        };
        if status != STATUS_SUCCESS {
            if is_transient_data_status(status) {
                return Ok(None);
            }
            return Err(format_pdh_error("read CPU counter", status));
        }

        if !is_valid_counter_status(value.CStatus) {
            if is_transient_data_status(value.CStatus) {
                return Ok(None);
            }
            return Err(format_pdh_error("read CPU counter value", value.CStatus));
        }

        let value = unsafe { value.Anonymous.doubleValue as f32 };
        Ok(normalize_percent(value))
    }

    fn read_gpu_percent(counter: PDH_HCOUNTER) -> Result<Option<f32>, String> {
        let mut buffer_size = 0u32;
        let mut item_count = 0u32;
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                std::ptr::null_mut(),
            )
        };
        if status != STATUS_SUCCESS && status != PDH_MORE_DATA {
            if is_transient_data_status(status) {
                return Ok(None);
            }
            return Err(format_pdh_error("query GPU counter size", status));
        }
        if buffer_size == 0 || item_count == 0 {
            return Ok(None);
        }

        let item_size = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        let mut items = vec![
            PDH_FMT_COUNTERVALUE_ITEM_W::default();
            ((buffer_size as usize) + item_size - 1) / item_size
        ];

        loop {
            let status = unsafe {
                PdhGetFormattedCounterArrayW(
                    counter,
                    PDH_FMT_DOUBLE,
                    &mut buffer_size,
                    &mut item_count,
                    items.as_mut_ptr(),
                )
            };
            if status == STATUS_SUCCESS {
                break;
            }
            if status == PDH_MORE_DATA {
                items.resize(
                    ((buffer_size as usize) + item_size - 1) / item_size,
                    PDH_FMT_COUNTERVALUE_ITEM_W::default(),
                );
                continue;
            }
            if is_transient_data_status(status) {
                return Ok(None);
            }
            return Err(format_pdh_error("read GPU counter array", status));
        }

        let busiest = items
            .iter()
            .take(item_count as usize)
            .filter_map(|item| {
                if !is_valid_counter_status(item.FmtValue.CStatus) {
                    return None;
                }
                let value = unsafe { item.FmtValue.Anonymous.doubleValue as f32 };
                normalize_percent(value)
            })
            .fold(None::<f32>, |current, value| {
                Some(current.map_or(value, |best| best.max(value)))
            });

        Ok(busiest)
    }

    fn normalize_percent(value: f32) -> Option<f32> {
        if value.is_finite() {
            Some(value.clamp(0.0, 100.0))
        } else {
            None
        }
    }

    fn is_valid_counter_status(status: u32) -> bool {
        matches!(status, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA)
    }

    fn is_transient_data_status(status: u32) -> bool {
        matches!(
            status,
            PDH_NO_DATA
                | PDH_INVALID_DATA
                | PDH_CSTATUS_INVALID_DATA
                | PDH_CSTATUS_ITEM_NOT_VALIDATED
                | PDH_CSTATUS_NO_INSTANCE
                | PDH_CSTATUS_NO_MACHINE
                | PDH_CSTATUS_NO_OBJECT
        )
    }

    fn format_pdh_error(action: &str, status: u32) -> String {
        format!("{action} failed (PDH 0x{status:08X})")
    }

    fn wide_string(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(iter::once(0)).collect()
    }
}

#[cfg(windows)]
use windows_backend::SystemTelemetry;

pub fn build_points(history: &[f32], width: f32, height: f32) -> Vec<EcgPoint> {
    if history.is_empty() || width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }

    if history.len() == 1 {
        let value = history[0].clamp(0.0, 100.0);
        return vec![EcgPoint {
            x: width / 2.0,
            y: height - (value / 100.0) * height,
        }];
    }

    let step_x = width / (history.len().saturating_sub(1) as f32);

    history
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let normalized = value.clamp(0.0, 100.0);
            EcgPoint {
                x: index as f32 * step_x,
                y: height - (normalized / 100.0) * height,
            }
        })
        .collect()
}
