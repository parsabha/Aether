//! Live hardware + FPS sampling for the in-game island overlay.
//!
//! FPS comes from a PresentMon console subprocess (ETW). Sensors use sysinfo,
//! NVML (NVIDIA), DXGI VRAM, and PDH GPU / thermal counters.

use parking_lot::Mutex;
use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sysinfo::System;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStats {
    pub fps: Option<f32>,
    pub cpu_usage: Option<f32>,
    pub gpu_usage: Option<f32>,
    pub cpu_temp_c: Option<f32>,
    pub vram_used_mb: Option<f32>,
    pub vram_total_mb: Option<f32>,
    /// Results-only extras (island ignores these).
    pub gpu_temp_c: Option<f32>,
    pub ram_used_mb: Option<f32>,
    pub ram_total_mb: Option<f32>,
    pub frame_time_ms: Option<f32>,
    pub gpu_power_w: Option<f32>,
}

static LATEST: Mutex<OverlayStats> = Mutex::new(OverlayStats {
    fps: None,
    cpu_usage: None,
    gpu_usage: None,
    cpu_temp_c: None,
    vram_used_mb: None,
    vram_total_mb: None,
    gpu_temp_c: None,
    ram_used_mb: None,
    ram_total_mb: None,
    frame_time_ms: None,
    gpu_power_w: None,
});
static RUNNING: AtomicBool = AtomicBool::new(false);
static TARGET_PID: AtomicU32 = AtomicU32::new(0);
static FPS_CACHE: Mutex<Option<f32>> = Mutex::new(None);
static APP_HANDLE: Mutex<Option<AppHandle>> = Mutex::new(None);

pub fn latest() -> OverlayStats {
    let mut s = LATEST.lock().clone();
    s.fps = *FPS_CACHE.lock();
    s
}

pub fn set_target_pid(pid: Option<u32>) {
    TARGET_PID.store(pid.unwrap_or(0), Ordering::Relaxed);
}

pub fn target_pid() -> u32 {
    TARGET_PID.load(Ordering::Relaxed)
}

pub fn ensure_started(app: &AppHandle) {
    *APP_HANDLE.lock() = Some(app.clone());
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    std::thread::Builder::new()
        .name("aether-overlay-stats".into())
        .spawn(move || sampler_loop(handle))
        .ok();
}

pub fn stop() {
    RUNNING.store(false, Ordering::SeqCst);
    TARGET_PID.store(0, Ordering::Relaxed);
    *FPS_CACHE.lock() = None;
    *LATEST.lock() = OverlayStats::default();
}

fn sampler_loop(app: AppHandle) {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    let mut sys = System::new();
    // Prime CPU usage (needs two samples).
    sys.refresh_cpu_usage();
    std::thread::sleep(Duration::from_millis(120));

    let mut nvml = NvmlSensors::open();
    let mut gpu_pdh = GpuPdh::open();
    let mut thermal = ThermalPdh::open();
    let mut present = PresentMon::new();
    let mut tick_n: u32 = 0;

    while RUNNING.load(Ordering::SeqCst) {
        let sample_ram = tick_n % 4 == 0;
        let in_game = TARGET_PID.load(Ordering::Relaxed) != 0;
        let tick = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let pid = TARGET_PID.load(Ordering::Relaxed);
            present.sync(pid, &app);

            // CPU PDH refresh is relatively heavy — half rate while gaming.
            if !in_game || tick_n % 2 == 0 {
                sys.refresh_cpu_usage();
            }
            // Memory less often — full refresh every tick is unnecessary for charts.
            let (ram_used, ram_total) = if sample_ram {
                sys.refresh_memory();
                (
                    Some((sys.used_memory() as f32) / (1024.0 * 1024.0)),
                    Some((sys.total_memory() as f32) / (1024.0 * 1024.0)),
                )
            } else {
                let s = LATEST.lock();
                (s.ram_used_mb, s.ram_total_mb)
            };

            let (gpu_nv, vram_nv, gpu_temp, gpu_power) = nvml.sample();
            // Prefer NVML — skip PDH/DXGI when we already have numbers (PDH is costly).
            let gpu = gpu_nv.or_else(|| {
                if in_game && tick_n % 2 == 1 {
                    None
                } else {
                    gpu_pdh.sample()
                }
            });

            let vram = vram_nv.or_else(|| {
                if in_game {
                    None
                } else {
                    dxgi_vram()
                }
            });
            let (vram_used, vram_total) = match vram {
                Some((u, t)) => (Some(u), Some(t)),
                None => (None, None),
            };

            // Prefer real CPU package sensors; fall back to GPU board temp for HUD.
            let cpu_temp = if in_game && tick_n % 2 == 1 {
                gpu_temp.or_else(|| thermal.sample())
            } else {
                thermal.sample().or(gpu_temp)
            };
            let fps = *FPS_CACHE.lock();
            let frame_time = fps.filter(|f| *f > 0.5).map(|f| 1000.0 / f);
            let cpu = Some(sys.global_cpu_usage().clamp(0.0, 100.0));

            OverlayStats {
                fps,
                cpu_usage: cpu,
                gpu_usage: gpu,
                cpu_temp_c: cpu_temp,
                vram_used_mb: vram_used,
                vram_total_mb: vram_total,
                gpu_temp_c: gpu_temp,
                ram_used_mb: ram_used,
                ram_total_mb: ram_total,
                frame_time_ms: frame_time,
                gpu_power_w: gpu_power,
            }
        }));

        tick_n = tick_n.wrapping_add(1);

        if let Ok(stats) = tick {
            let changed = {
                let mut latest = LATEST.lock();
                let ch = latest.fps != stats.fps
                    || latest.cpu_usage != stats.cpu_usage
                    || latest.gpu_usage != stats.gpu_usage
                    || latest.cpu_temp_c != stats.cpu_temp_c
                    || latest.vram_used_mb != stats.vram_used_mb;
                *latest = stats.clone();
                ch
            };
            // Don't spam the WebView IPC every tick when numbers are flat.
            if changed || tick_n % 2 == 0 {
                let _ = app.emit("overlay:stats", &stats);
            }
            if let Some(state) = app.try_state::<crate::games::AppState>() {
                crate::session_telemetry::record_sample(
                    &state.db,
                    crate::db::SessionSample {
                        t_ms: 0,
                        fps: stats.fps,
                        cpu_usage: stats.cpu_usage,
                        gpu_usage: stats.gpu_usage,
                        cpu_temp_c: stats.cpu_temp_c,
                        vram_used_mb: stats.vram_used_mb,
                        vram_total_mb: stats.vram_total_mb,
                        gpu_temp_c: stats.gpu_temp_c,
                        ram_used_mb: stats.ram_used_mb,
                        ram_total_mb: stats.ram_total_mb,
                        frame_time_ms: stats.frame_time_ms,
                        gpu_power_w: stats.gpu_power_w,
                    },
                );
            }
        }

        // Quiet HUD while a game is up (~1.25 Hz). Desktop idle can stay snappier.
        let sleep_ms = if TARGET_PID.load(Ordering::Relaxed) != 0 {
            800
        } else {
            250
        };
        std::thread::sleep(Duration::from_millis(sleep_ms));
    }

    present.stop();
}

/* —— PresentMon FPS —— */

struct PresentMon {
    child: Option<Child>,
    pid: u32,
    reader_stop: Option<Arc<AtomicBool>>,
}

impl PresentMon {
    fn new() -> Self {
        Self {
            child: None,
            pid: 0,
            reader_stop: None,
        }
    }

    fn sync(&mut self, pid: u32, app: &AppHandle) {
        if pid == 0 {
            self.stop();
            *FPS_CACHE.lock() = None;
            return;
        }
        // Prefer RTSS shared memory when available (Afterburner / RTSS).
        // It's far cheaper than PresentMon ETW and avoids Vulkan present hitching.
        if let Some(fps) = read_rtss_fps(pid) {
            *FPS_CACHE.lock() = Some(fps);
            if self.child.is_some() {
                self.stop();
            }
            return;
        }
        if self.pid == pid && self.child.is_some() {
            if let Some(child) = self.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    // Process ended — clear so we can restart on next tick.
                    let _ = status;
                    self.cleanup_child();
                } else {
                    // Keep PresentMon at IDLE — Windows can bump priority over time.
                    crate::game_perf::set_pid_idle(child.id());
                    return;
                }
            }
        }
        if self.pid != pid {
            self.stop();
        }
        let Some(exe) = find_presentmon(app) else {
            return;
        };

        let mut cmd = Command::new(&exe);
        // Stream CSV on stdout. PresentMon opens --output_file exclusively
        // (ERROR_SHARING_VIOLATION), so a file tailer can never read live FPS.
        // Do NOT pass --restart_as_admin: it respawns and drops our pipes.
        // Aether is already elevated in release builds; the child inherits ETW rights.
        cmd.args([
            "--process_id",
            &pid.to_string(),
            "--output_stdout",
            "--v2_metrics",
            "--exclude_dropped",
            "--no_console_stats",
            // Lighter ETW: FPS only needs present timing, not GPU/input tracing.
            // Full GPU/input tracking is a common cause of "feels laggy" with no FPS drop.
            "--no_track_gpu",
            "--no_track_input",
            "--stop_existing_session",
            "--session_name",
            "AetherOverlay",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let Ok(mut child) = cmd.spawn() else {
            return;
        };
        // PresentMon ETW is sensitive — keep it idle so the game keeps the CPU/GPU.
        crate::game_perf::set_pid_idle(child.id());
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                let mut err = String::new();
                let _ = std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut err);
                if !err.trim().is_empty() {
                    let log = std::env::temp_dir().join("aether-presentmon.log");
                    let _ = std::fs::write(&log, err);
                }
            });
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        if let Some(stdout) = child.stdout.take() {
            std::thread::Builder::new()
                .name("aether-presentmon-out".into())
                .spawn(move || read_presentmon_csv(BufReader::new(stdout), stop2))
                .ok();
        }
        self.reader_stop = Some(stop);
        self.child = Some(child);
        self.pid = pid;
    }

    fn cleanup_child(&mut self) {
        if let Some(stop) = self.reader_stop.take() {
            stop.store(true, Ordering::SeqCst);
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.pid = 0;
    }

    fn stop(&mut self) {
        self.cleanup_child();
    }
}

impl Drop for PresentMon {
    fn drop(&mut self) {
        self.stop();
    }
}

fn find_presentmon(app: &AppHandle) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        candidates.push(dir.join("PresentMon.exe"));
        candidates.push(dir.join("resources").join("PresentMon.exe"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("PresentMon.exe"));
            candidates.push(dir.join("resources").join("PresentMon.exe"));
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/PresentMon.exe"));
    candidates.into_iter().find(|p| p.is_file())
}

fn read_presentmon_csv(reader: impl BufRead, stop: Arc<AtomicBool>) {
    let mut header: Option<Vec<String>> = None;
    let mut ms_idx: Option<usize> = None;
    let mut window: Vec<(f64, InstantStamp)> = Vec::new();

    for line in reader.lines() {
        if stop.load(Ordering::Relaxed) || !RUNNING.load(Ordering::Relaxed) {
            break;
        }
        let Ok(line) = line else {
            break;
        };
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with("Started recording") || line.starts_with("Stopped recording")
        {
            continue;
        }
        if header.is_none() {
            let lower = line.to_ascii_lowercase();
            if !(lower.contains("application")
                || lower.contains("msbetween")
                || lower.contains("processid")
                || lower.contains("frametime"))
            {
                continue;
            }
            let cols: Vec<String> = line.split(',').map(|s| s.trim().to_string()).collect();
            ms_idx = cols.iter().position(|c| {
                let l = c.to_ascii_lowercase();
                l == "msbetweenpresents"
                    || l == "msbetweendisplaychange"
                    || l == "frametime"
                    || l.contains("betweenpresents")
            });
            header = Some(cols);
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        let now = InstantStamp::now();
        let Some(idx) = ms_idx else {
            continue;
        };
        let Some(raw) = cols.get(idx) else {
            continue;
        };
        let Ok(ms) = raw.trim().parse::<f64>() else {
            continue;
        };
        if !(0.05..=500.0).contains(&ms) {
            continue;
        }
        window.push((ms, now));
        // Short window so the HUD reacts within ~200–300ms instead of a full second.
        window.retain(|(_, t)| now.elapsed_ms(t) < 280);
        if window.is_empty() {
            continue;
        }
        let avg = window.iter().map(|(m, _)| *m).sum::<f64>() / window.len() as f64;
        if avg > 0.0 {
            let fps = ((1000.0 / avg) as f32).clamp(1.0, 1000.0);
            *FPS_CACHE.lock() = Some(fps);
            // Do NOT emit overlay:stats per present here — that floods the island
            // WebView at 60–120 Hz and feels like input/camera lag with no FPS drop.
            // The sampler loop (~8 Hz) pushes HUD updates instead.
        }
    }
}

#[derive(Clone, Copy)]
struct InstantStamp(std::time::Instant);

impl InstantStamp {
    fn now() -> Self {
        Self(std::time::Instant::now())
    }
    fn elapsed_ms(&self, earlier: &Self) -> u128 {
        self.0.duration_since(earlier.0).as_millis()
    }
}

/// MSI Afterburner / RTSS shared memory FPS (optional).
#[cfg(windows)]
fn read_rtss_fps(target_pid: u32) -> Option<f32> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Memory::{
        MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS,
    };

    // RTSSSharedMemoryV2 layout (simplified): header then entries.
    // https://github.com/Alexey-T/RTSS_SharedMemory_examples
    const ENTRY_SIZE: usize = 528; // typical for V2.x app entries with name[260]
    unsafe {
        let name: Vec<u16> = "RTSSSharedMemoryV2\0".encode_utf16().collect();
        let mapping = OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(name.as_ptr())).ok()?;
        let view: MEMORY_MAPPED_VIEW_ADDRESS = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0);
        if view.Value.is_null() {
            let _ = CloseHandle(mapping);
            return None;
        }
        let base = view.Value as *const u8;
        // dwSignature 'RTSS', dwVersion, dwAppEntrySize, dwAppArrOffset, dwAppArrSize
        let signature = std::ptr::read_unaligned(base as *const u32);
        if signature != 0x5254_5353 {
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(mapping);
            return None;
        }
        let app_entry_size = std::ptr::read_unaligned(base.add(12) as *const u32) as usize;
        let app_arr_offset = std::ptr::read_unaligned(base.add(16) as *const u32) as usize;
        let app_arr_size = std::ptr::read_unaligned(base.add(20) as *const u32) as usize;
        let entry_size = if app_entry_size > 0 { app_entry_size } else { ENTRY_SIZE };
        let mut best: Option<f32> = None;
        for i in 0..app_arr_size.min(256) {
            let entry = base.add(app_arr_offset + i * entry_size);
            let process_id = std::ptr::read_unaligned(entry as *const u32);
            if process_id == 0 {
                continue;
            }
            if target_pid != 0 && process_id != target_pid {
                continue;
            }
            // dwTime0, dwTime1, dwFrames typically after name — offsets vary by version.
            // Common V2.1+: process_id @0, name @4 (260 WCHAR? or CHAR[260])
            // We'll try ANSI name layout: pid(4) + name(260) + ... time0/time1/frames
            let time0 = std::ptr::read_unaligned(entry.add(268) as *const u32);
            let time1 = std::ptr::read_unaligned(entry.add(272) as *const u32);
            let frames = std::ptr::read_unaligned(entry.add(276) as *const u32);
            let dt = time1.wrapping_sub(time0);
            if dt > 0 && frames > 0 {
                let fps = frames as f32 * 1000.0 / dt as f32;
                if (1.0..1000.0).contains(&fps) {
                    best = Some(best.map_or(fps, |b| b.max(fps)));
                }
            }
        }
        let _ = UnmapViewOfFile(view);
        let _ = CloseHandle(mapping);
        best
    }
}

#[cfg(not(windows))]
fn read_rtss_fps(_target_pid: u32) -> Option<f32> {
    None
}

/* —— NVML (NVIDIA) —— */

struct NvmlSensors {
    #[cfg(windows)]
    inner: Option<Arc<nvml_wrapper::Nvml>>,
}

impl NvmlSensors {
    fn open() -> Self {
        #[cfg(windows)]
        {
            let inner = nvml_wrapper::Nvml::init().ok().map(Arc::new);
            return Self { inner };
        }
        #[cfg(not(windows))]
        Self {}
    }

    fn sample(&mut self) -> (Option<f32>, Option<(f32, f32)>, Option<f32>, Option<f32>) {
        #[cfg(windows)]
        {
            let Some(nvml) = self.inner.as_ref() else {
                return (None, None, None, None);
            };
            let Ok(device) = nvml.device_by_index(0) else {
                return (None, None, None, None);
            };
            let gpu = device
                .utilization_rates()
                .ok()
                .map(|u| u.gpu as f32);
            let vram = device.memory_info().ok().map(|m| {
                let used = m.used as f32 / (1024.0 * 1024.0);
                let total = m.total as f32 / (1024.0 * 1024.0);
                (used, total)
            });
            let temp = device
                .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                .ok()
                .map(|t| t as f32);
            let power = device
                .power_usage()
                .ok()
                .map(|mw| mw as f32 / 1000.0);
            return (gpu, vram, temp, power);
        }
        #[cfg(not(windows))]
        (None, None, None, None)
    }
}

/* —— DXGI VRAM —— */

#[cfg(windows)]
fn dxgi_vram() -> Option<(f32, f32)> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIAdapter3, IDXGIFactory1, DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
        DXGI_QUERY_VIDEO_MEMORY_INFO,
    };

    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let adapter = factory.EnumAdapters1(0).ok()?;
        let adapter3: IDXGIAdapter3 = adapter.cast().ok()?;
        let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
        adapter3
            .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info)
            .ok()?;
        if info.Budget == 0 {
            return None;
        }
        let used = info.CurrentUsage as f32 / (1024.0 * 1024.0);
        let total = info.Budget as f32 / (1024.0 * 1024.0);
        Some((used, total))
    }
}

#[cfg(not(windows))]
fn dxgi_vram() -> Option<(f32, f32)> {
    None
}

/* —— PDH helpers —— */

struct PdhQuery {
    handle: isize,
    counter: isize,
}

#[cfg(windows)]
fn pdh_collect_doubles(query: &PdhQuery) -> Option<Vec<(String, f64)>> {
    use windows::Win32::System::Performance::{
        PdhCollectQueryData, PdhGetFormattedCounterArrayW, PDH_FMT_COUNTERVALUE_ITEM_W,
        PDH_FMT_DOUBLE, PDH_MORE_DATA,
    };
    unsafe {
        if PdhCollectQueryData(query.handle) != 0 {
            return None;
        }
        let mut size = 0u32;
        let mut count = 0u32;
        let st = PdhGetFormattedCounterArrayW(
            query.counter,
            PDH_FMT_DOUBLE,
            &mut size,
            &mut count,
            None,
        );
        // PDH writes instance names into the *same* buffer after the item array.
        // Allocating only `count` structs is too small and corrupts the heap.
        if st != PDH_MORE_DATA || size == 0 {
            return None;
        }
        let mut raw = vec![0u8; size as usize];
        let mut size2 = size;
        let mut count2 = count;
        let status = PdhGetFormattedCounterArrayW(
            query.counter,
            PDH_FMT_DOUBLE,
            &mut size2,
            &mut count2,
            Some(raw.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W),
        );
        if status != 0 || count2 == 0 {
            return None;
        }
        let items = std::slice::from_raw_parts(
            raw.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
            count2 as usize,
        );
        let mut out = Vec::with_capacity(count2 as usize);
        for item in items {
            let name = if item.szName.is_null() {
                String::new()
            } else {
                item.szName.to_string().unwrap_or_default()
            };
            let v = item.FmtValue.Anonymous.doubleValue;
            if v.is_finite() {
                out.push((name, v));
            }
        }
        Some(out)
    }
}

/* —— PDH GPU utilization —— */

struct GpuPdh {
    #[cfg(windows)]
    query: Option<PdhQuery>,
}

impl GpuPdh {
    fn open() -> Self {
        #[cfg(windows)]
        {
            use windows::core::w;
            use windows::Win32::System::Performance::{PdhAddEnglishCounterW, PdhOpenQueryW};
            unsafe {
                let mut query: isize = 0;
                if PdhOpenQueryW(None, 0, &mut query) != 0 {
                    return Self { query: None };
                }
                let mut counter: isize = 0;
                let status = PdhAddEnglishCounterW(
                    query,
                    w!("\\GPU Engine(*)\\Utilization Percentage"),
                    0,
                    &mut counter,
                );
                if status != 0 {
                    let _ = windows::Win32::System::Performance::PdhCloseQuery(query);
                    return Self { query: None };
                }
                // Warm up.
                let _ = windows::Win32::System::Performance::PdhCollectQueryData(query);
                return Self {
                    query: Some(PdhQuery {
                        handle: query,
                        counter,
                    }),
                };
            }
        }
        #[cfg(not(windows))]
        Self {}
    }

    fn sample(&mut self) -> Option<f32> {
        #[cfg(windows)]
        {
            let q = self.query.as_ref()?;
            let rows = pdh_collect_doubles(q)?;
            let mut best = 0.0f64;
            for (name, v) in &rows {
                let lower = name.to_ascii_lowercase();
                if lower.contains("engtype_3d") || lower.contains("engtype_graphics") {
                    best = best.max(*v);
                }
            }
            if best <= 0.0 {
                for (_, v) in &rows {
                    best = best.max(*v);
                }
            }
            return Some(best.clamp(0.0, 100.0) as f32);
        }
        #[cfg(not(windows))]
        None
    }
}

impl Drop for GpuPdh {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(q) = self.query.take() {
            unsafe {
                let _ = windows::Win32::System::Performance::PdhCloseQuery(q.handle);
            }
        }
    }
}

/* —— PDH thermal zones (CPU temp best-effort) —— */

struct ThermalPdh {
    #[cfg(windows)]
    query: Option<PdhQuery>,
}

impl ThermalPdh {
    fn open() -> Self {
        #[cfg(windows)]
        {
            use windows::core::w;
            use windows::Win32::System::Performance::PdhAddEnglishCounterW;
            use windows::Win32::System::Performance::PdhOpenQueryW;
            unsafe {
                let mut query: isize = 0;
                if PdhOpenQueryW(None, 0, &mut query) != 0 {
                    return Self { query: None };
                }
                let mut counter: isize = 0;
                let paths = [
                    w!("\\Thermal Zone Information(*)\\Temperature"),
                    w!("\\Thermal Zone Information(*)\\High Precision Temperature"),
                ];
                let mut ok = false;
                for path in paths {
                    let status = PdhAddEnglishCounterW(query, path, 0, &mut counter);
                    if status == 0 {
                        ok = true;
                        break;
                    }
                }
                if !ok {
                    let _ = windows::Win32::System::Performance::PdhCloseQuery(query);
                    return Self { query: None };
                }
                let _ = windows::Win32::System::Performance::PdhCollectQueryData(query);
                return Self {
                    query: Some(PdhQuery {
                        handle: query,
                        counter,
                    }),
                };
            }
        }
        #[cfg(not(windows))]
        Self {}
    }

    fn sample(&mut self) -> Option<f32> {
        #[cfg(windows)]
        {
            let q = self.query.as_ref()?;
            let rows = pdh_collect_doubles(q)?;
            let mut cpu_best: Option<f32> = None;
            let mut any_best: Option<f32> = None;
            for (name, raw) in rows {
                if raw <= 0.0 {
                    continue;
                }
                let Some(c) = normalize_temp_c(raw) else {
                    continue;
                };
                // ACPI "TZ*" ambient zones often sit around ~25–30°C forever — skip those.
                let lower = name.to_ascii_lowercase();
                let looks_cpu = lower.contains("cpu")
                    || lower.contains("package")
                    || lower.contains("core")
                    || lower.contains("processor");
                let looks_ambient = lower.contains("\\tz")
                    || lower.contains("_tz")
                    || lower.contains("thermal zone")
                    || lower.contains("acpi");
                if looks_cpu {
                    cpu_best = Some(cpu_best.map_or(c, |b| b.max(c)));
                } else if !looks_ambient && c >= 40.0 {
                    // Only keep non-ambient anonymous sensors if they look like load temps.
                    any_best = Some(any_best.map_or(c, |b| b.max(c)));
                }
            }
            return cpu_best.or(any_best);
        }
        #[cfg(not(windows))]
        None
    }
}

impl Drop for ThermalPdh {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(q) = self.query.take() {
            unsafe {
                let _ = windows::Win32::System::Performance::PdhCloseQuery(q.handle);
            }
        }
    }
}

fn normalize_temp_c(raw: f64) -> Option<f32> {
    // High Precision Temperature is often tenths of Kelvin.
    // Plain Temperature may already be Celsius on some systems.
    let c = if raw > 200.0 && raw < 500.0 {
        // Kelvin
        raw - 273.15
    } else if raw >= 500.0 {
        // Tenths of Kelvin
        (raw / 10.0) - 273.15
    } else {
        raw
    };
    if (15.0..=125.0).contains(&c) {
        Some(c as f32)
    } else {
        None
    }
}

#[tauri::command]
pub fn overlay_stats() -> OverlayStats {
    latest()
}
