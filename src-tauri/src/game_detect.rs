//! Detect games for the in-game overlay even when not launched from Aether.

use crate::games::{AppState, RunningGame};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Full process scans are expensive — do them rarely while a game is already tracked.
static DETECT_TICK: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone)]
pub struct DetectedGame {
    pub pid: u32,
    pub game_id: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn exe_key(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase()
}

fn dir_key(path: &Path) -> Option<String> {
    path.parent()
        .map(|p| p.to_string_lossy().to_ascii_lowercase())
}

/// Snapshot of running process id → exe path (best effort).
#[cfg(windows)]
fn running_processes() -> HashMap<u32, PathBuf> {
    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::System::ProcessStatus::EnumProcesses;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let mut out = HashMap::new();
    let mut pids = vec![0u32; 2048];
    let mut needed = 0u32;
    unsafe {
        loop {
            let bytes = (pids.len() * 4) as u32;
            if EnumProcesses(pids.as_mut_ptr(), bytes, &mut needed).is_err() {
                return out;
            }
            let count = (needed as usize) / 4;
            if count < pids.len() {
                pids.truncate(count);
                break;
            }
            pids.resize(pids.len() * 2, 0);
        }
        let me = std::process::id();
        for &pid in &pids {
            if pid == 0 || pid == me {
                continue;
            }
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                continue;
            };
            let mut buf = [0u16; MAX_PATH as usize];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut size,
            );
            let _ = CloseHandle(handle);
            if ok.is_err() || size == 0 {
                continue;
            }
            let path = String::from_utf16_lossy(&buf[..size as usize]);
            out.insert(pid, PathBuf::from(path));
        }
    }
    out
}

#[cfg(not(windows))]
fn running_processes() -> HashMap<u32, PathBuf> {
    HashMap::new()
}

#[cfg(windows)]
fn foreground_fullscreen_pid() -> Option<u32> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() || !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == std::process::id() {
            return None;
        }
        let mut wr = RECT::default();
        if GetWindowRect(hwnd, &mut wr).is_err() {
            return None;
        }
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return None;
        }
        let mr = mi.rcMonitor;
        let ww = (wr.right - wr.left).abs();
        let wh = (wr.bottom - wr.top).abs();
        let mw = (mr.right - mr.left).abs();
        let mh = (mr.bottom - mr.top).abs();
        if mw <= 0 || mh <= 0 {
            return None;
        }
        // Normal fullscreen / borderless: covers ~95%+ of the monitor.
        if ww * 100 >= mw * 95 && wh * 100 >= mh * 95 {
            return Some(pid);
        }
        None
    }
}

#[cfg(not(windows))]
fn foreground_fullscreen_pid() -> Option<u32> {
    None
}

fn is_blocked_exe(name: &str) -> bool {
    const BLOCK: &[&str] = &[
        "explorer.exe",
        "searchhost.exe",
        "shellexperiencehost.exe",
        "applicationframehost.exe",
        "systemsettings.exe",
        "taskmgr.exe",
        "dwm.exe",
        "winlogon.exe",
        "csrss.exe",
        "aether.exe",
        "presentmon.exe",
        "code.exe",
        "cursor.exe",
        "devenv.exe",
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "discord.exe",
        "spotify.exe",
        "steamwebhelper.exe",
        "nvidia share.exe",
        "textinputhost.exe",
    ];
    BLOCK.iter().any(|b| name == *b)
}

/// Launchers / bootstrappers that rarely present frames (Rockstar, etc.).
fn is_launcher_stub(name: &str) -> bool {
    const STUBS: &[&str] = &[
        "launcher.exe",
        "launchpad.exe",
        "playrdr2.exe",
        "playgtav.exe",
        "playrdr.exe",
        "rockstarsteamhelper.exe",
        "socialclubhelper.exe",
        "socialclub.exe",
        "upc.exe",
        "ubisoftconnect.exe",
        "ubisoftgamelauncher.exe",
        "epicgameslauncher.exe",
        "galaxyclient.exe",
        "origin.exe",
        "eadesktop.exe",
        "easports.exe",
        "battle.net.exe",
        "agent.exe",
        "steam.exe",
    ];
    let n = name.to_ascii_lowercase();
    STUBS.iter().any(|s| n == *s)
        || n.ends_with("launcher.exe")
        || n.contains("crashhandler")
        || n.contains("errorhandler")
}

fn is_helper_exe(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("helper")
        || n.contains("crash")
        || n.contains("error")
        || n.contains("cef")
        || n.contains("webview")
        || n.contains("service")
}

/// Among processes in the game's install folder, pick the one that actually presents
/// frames (e.g. RDR2.exe instead of Rockstar Launcher.exe). Critical for Vulkan titles
/// that boot through a launcher stub.
fn resolve_game_renderer_pid(
    registered_exe: &Path,
    tracked_pid: u32,
    procs: &HashMap<u32, PathBuf>,
) -> Option<u32> {
    let Some(dir) = dir_key(registered_exe) else {
        return (tracked_pid != 0).then_some(tracked_pid);
    };

    let mut candidates: Vec<(u32, String)> = Vec::new();
    for (pid, path) in procs {
        if dir_key(path).as_deref() != Some(dir.as_str()) {
            continue;
        }
        let name = exe_key(&path.to_string_lossy());
        if is_blocked_exe(&name) {
            continue;
        }
        candidates.push((*pid, name));
    }
    if candidates.is_empty() {
        return (tracked_pid != 0).then_some(tracked_pid);
    }

    // Fullscreen foreground in this install folder wins (Vulkan borderless FG).
    if let Some(fg) = foreground_fullscreen_pid() {
        if candidates.iter().any(|(p, _)| *p == fg) {
            return Some(fg);
        }
    }

    let reg_name = exe_key(&registered_exe.to_string_lossy());
    if !is_launcher_stub(&reg_name) {
        if let Some((pid, _)) = candidates.iter().find(|(_, n)| *n == reg_name) {
            return Some(*pid);
        }
    }

    let mut non_stubs: Vec<(u32, String)> = candidates
        .iter()
        .filter(|(_, n)| !is_launcher_stub(n) && !is_helper_exe(n))
        .cloned()
        .collect();
    if non_stubs.is_empty() {
        non_stubs = candidates
            .iter()
            .filter(|(_, n)| !is_launcher_stub(n))
            .cloned()
            .collect();
    }
    if non_stubs.is_empty() {
        if tracked_pid != 0 && candidates.iter().any(|(p, _)| *p == tracked_pid) {
            return Some(tracked_pid);
        }
        return Some(candidates[0].0);
    }
    if non_stubs.len() == 1 {
        return Some(non_stubs[0].0);
    }
    if non_stubs.iter().any(|(p, _)| *p == tracked_pid) && !is_launcher_stub(
        &candidates
            .iter()
            .find(|(p, _)| *p == tracked_pid)
            .map(|(_, n)| n.clone())
            .unwrap_or_default(),
    ) {
        return Some(tracked_pid);
    }
    // Prefer the process whose name looks like the game folder (e.g. rdr2).
    if let Some(folder) = Path::new(&dir).file_name().and_then(|s| s.to_str()) {
        let folder = folder.to_ascii_lowercase().replace(' ', "");
        if let Some((pid, _)) = non_stubs.iter().find(|(_, n)| {
            let stem = n.trim_end_matches(".exe").replace([' ', '_', '-'], "");
            folder.contains(&stem) || stem.contains(&folder)
        }) {
            return Some(*pid);
        }
    }
    Some(non_stubs[0].0)
}

fn game_id_for_path(
    path: &Path,
    by_exe: &HashMap<String, String>,
    by_dir: &HashMap<String, String>,
) -> Option<String> {
    let key = exe_key(&path.to_string_lossy());
    if let Some(id) = by_exe.get(&key) {
        return Some(id.clone());
    }
    dir_key(path).and_then(|d| by_dir.get(&d).cloned())
}

fn index_games(games: &[crate::db::Game]) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut by_exe: HashMap<String, String> = HashMap::new();
    let mut by_dir: HashMap<String, String> = HashMap::new();
    for g in games {
        if let Some(exe) = g.exe_path.as_deref() {
            if exe.to_ascii_lowercase().starts_with("steam://") {
                continue;
            }
            by_exe.insert(exe_key(exe), g.id.clone());
            if let Some(dir) = dir_key(Path::new(exe)) {
                by_dir.entry(dir).or_insert_with(|| g.id.clone());
            }
        }
    }
    (by_exe, by_dir)
}

/// Find the best overlay target: library game process, else fullscreen FG if it matches library.
pub fn detect_overlay_target(state: &AppState) -> Option<DetectedGame> {
    let games = state.db.list_games().ok()?;
    let (by_exe, by_dir) = index_games(&games);
    let has_pending = state.running.lock().values().any(|rg| rg.pid == 0);
    if by_exe.is_empty() && !has_pending {
        return None;
    }

    let procs = running_processes();

    // Prefer a game already tracked as launched from Aether — but upgrade launcher → renderer.
    {
        let running = state.running.lock();
        for (id, rg) in running.iter() {
            if rg.pid == 0 {
                continue;
            }
            if let Some(g) = games.iter().find(|g| g.id == *id) {
                if let Some(exe) = g.exe_path.as_deref() {
                    if exe.to_ascii_lowercase().starts_with("steam://") {
                        return Some(DetectedGame {
                            pid: rg.pid,
                            game_id: Some(id.clone()),
                        });
                    }
                    if let Some(pid) = resolve_game_renderer_pid(Path::new(exe), rg.pid, &procs) {
                        return Some(DetectedGame {
                            pid,
                            game_id: Some(id.clone()),
                        });
                    }
                }
            }
            return Some(DetectedGame {
                pid: rg.pid,
                game_id: Some(id.clone()),
            });
        }
    }

    // Pending Steam/protocol launch (pid=0): bind a fullscreen game process so
    // URI-only library entries still get overlay + playtime.
    {
        let pending: Vec<String> = {
            let running = state.running.lock();
            running
                .iter()
                .filter(|(_, rg)| rg.pid == 0)
                .map(|(id, _)| id.clone())
                .collect()
        };
        if pending.len() == 1 {
            let game_id = pending[0].clone();
            let g = games.iter().find(|g| g.id == game_id);
            let exe = g.and_then(|g| g.exe_path.as_deref());
            let is_protocol = exe
                .map(|e| e.to_ascii_lowercase().starts_with("steam://"))
                .unwrap_or(false);
            let is_steam = is_protocol
                || exe
                    .map(|e| crate::steam::looks_like_steam_game(e))
                    .unwrap_or(false);
            if is_steam {
                if let Some(fg) = foreground_fullscreen_pid() {
                    if let Some(path) = procs.get(&fg) {
                        let key = exe_key(&path.to_string_lossy());
                        if !is_blocked_exe(&key)
                            && !is_launcher_stub(&key)
                            && !is_helper_exe(&key)
                        {
                            return Some(DetectedGame {
                                pid: fg,
                                game_id: Some(game_id),
                            });
                        }
                    }
                }
            }
        }
    }

    // Match library exe OR any process in a registered game install folder.
    for (pid, path) in &procs {
        let key = exe_key(&path.to_string_lossy());
        if is_blocked_exe(&key) {
            continue;
        }
        if let Some(game_id) = game_id_for_path(path, &by_exe, &by_dir) {
            if let Some(g) = games.iter().find(|g| g.id == game_id) {
                if let Some(exe) = g.exe_path.as_deref() {
                    if let Some(best) = resolve_game_renderer_pid(Path::new(exe), *pid, &procs) {
                        return Some(DetectedGame {
                            pid: best,
                            game_id: Some(game_id),
                        });
                    }
                }
            }
            return Some(DetectedGame {
                pid: *pid,
                game_id: Some(game_id),
            });
        }
    }

    // Foreground fullscreen window whose exe/dir is in the library.
    if let Some(fg) = foreground_fullscreen_pid() {
        if let Some(path) = procs.get(&fg) {
            let key = exe_key(&path.to_string_lossy());
            if !is_blocked_exe(&key) {
                if let Some(game_id) = game_id_for_path(path, &by_exe, &by_dir) {
                    return Some(DetectedGame {
                        pid: fg,
                        game_id: Some(game_id),
                    });
                }
            }
        }
    }

    None
}

/// Prefer the fullscreen/renderer process for FPS (not the Rockstar/Epic launcher stub).
pub fn detect_presentmon_pid(state: &AppState) -> Option<u32> {
    let games = state.db.list_games().ok()?;
    let procs = running_processes();

    {
        let running = state.running.lock();
        for (id, rg) in running.iter() {
            if let Some(g) = games.iter().find(|g| g.id == *id) {
                if let Some(exe) = g.exe_path.as_deref() {
                    if let Some(pid) = resolve_game_renderer_pid(Path::new(exe), rg.pid, &procs) {
                        return Some(pid);
                    }
                }
            }
            if rg.pid != 0 {
                return Some(rg.pid);
            }
        }
    }

    detect_overlay_target(state).map(|d| d.pid)
}

/// Sync externally-detected library games into `state.running` so the island UI lights up.
/// Returns the overlay / PresentMon PID if any.
///
/// While a session is already tracked, skip `EnumProcesses` most of the time —
/// that scan hitchs games (esp. Vulkan) far more than FPS overlays do.
pub fn sync_detected_running(state: &AppState) -> Option<u32> {
    let tick = DETECT_TICK.fetch_add(1, Ordering::Relaxed);
    let (has_live_pid, has_pending) = {
        let running = state.running.lock();
        let live = running.values().any(|rg| rg.pid != 0);
        let pending = running.values().any(|rg| rg.pid == 0);
        (live, pending)
    };
    // Pending Steam/launcher handoff: scan every poll so we bind the real process quickly.
    // Live session: ~every 6th poll (~24s at 4s interval) to upgrade launcher→renderer.
    if has_live_pid && !has_pending && tick % 6 != 0 {
        let running = state.running.lock();
        return running.values().find(|rg| rg.pid != 0).map(|rg| rg.pid);
    }
    // No session yet: scan at most every other poll (~8s) for external launches.
    if !has_live_pid && !has_pending && tick % 2 != 0 {
        return None;
    }

    let Some(det) = detect_overlay_target(state) else {
        return None;
    };

    if let Some(game_id) = det.game_id.clone() {
        let existing = {
            let running = state.running.lock();
            running
                .get(&game_id)
                .map(|e| (e.session_id.clone(), e.started_at, e.pid, e.wait_until))
        };
        if let Some((session_id, started_at, _old_pid, _wait_until)) = existing {
            crate::session_telemetry::resume_session(&session_id, &game_id, started_at);
            let mut running = state.running.lock();
            if let Some(entry) = running.get_mut(&game_id) {
                // Keep the renderer PID fresh (Launcher → RDR2.exe, Steam handoff, etc.).
                if entry.pid != det.pid {
                    entry.pid = det.pid;
                }
            }
        } else {
            let (session_id, started) =
                crate::session_telemetry::begin_session(&state.db, &game_id).unwrap_or_else(|_| {
                    (uuid::Uuid::new_v4().to_string(), now_ms())
                });
            let now = now_ms();
            state.running.lock().insert(
                game_id,
                RunningGame {
                    started_at: started,
                    pid: det.pid,
                    session_id,
                    wait_until: now + 45_000,
                },
            );
        }
    }

    Some(det.pid)
}

/// Refresh all library matches (may find several; overlay uses the first).
#[allow(dead_code)]
pub fn detect_all_library_pids(state: &AppState) -> Vec<DetectedGame> {
    let Ok(games) = state.db.list_games() else {
        return Vec::new();
    };
    let procs = running_processes();
    let (by_exe, by_dir) = index_games(&games);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (pid, path) in &procs {
        let key = exe_key(&path.to_string_lossy());
        if is_blocked_exe(&key) {
            continue;
        }
        if let Some(id) = game_id_for_path(path, &by_exe, &by_dir) {
            if seen.insert(id.clone()) {
                let best = games
                    .iter()
                    .find(|g| g.id == id)
                    .and_then(|g| g.exe_path.as_deref())
                    .and_then(|exe| resolve_game_renderer_pid(Path::new(exe), *pid, &procs))
                    .unwrap_or(*pid);
                out.push(DetectedGame {
                    pid: best,
                    game_id: Some(id),
                });
            }
        }
    }
    out
}
