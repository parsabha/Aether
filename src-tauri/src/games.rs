use crate::db::{Db, Game, ScreenshotDto};
pub use crate::db::{GameDto, Settings};
use crate::media::{self, is_motion};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

pub struct AppState {
    pub db: Db,
    pub running: Mutex<HashMap<String, RunningGame>>,
    pub notifications: Mutex<Vec<crate::island_feed::IslandNotification>>,
}

pub struct RunningGame {
    pub started_at: i64,
    pub pid: u32,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn path_exists(p: &Option<String>) -> bool {
    p.as_ref().map(|s| Path::new(s).exists()).unwrap_or(false)
}

fn asset_url(path: &Option<String>) -> Option<String> {
    // Absolute filesystem path — frontend converts via convertFileSrc.
    path.as_ref().filter(|p| Path::new(p).exists()).cloned()
}

fn cover_urls(g: &Game) -> (Option<String>, String) {
    if g.cover_webm.as_ref().map(|p| Path::new(p).exists()).unwrap_or(false) {
        return (asset_url(&g.cover_webm), "video".into());
    }
    if let Some(c) = &g.cover {
        if Path::new(c).exists() {
            if is_motion(Path::new(c)) {
                // Prefer poster while waiting for transcode; never feed raw GIF to UI when poster exists
                if g.cover_poster.as_ref().map(|p| Path::new(p).exists()).unwrap_or(false) {
                    return (asset_url(&g.cover_poster), "image".into());
                }
                return (asset_url(&g.cover), "gif".into());
            }
            return (asset_url(&g.cover), "image".into());
        }
    }
    if g.cover_poster.as_ref().map(|p| Path::new(p).exists()).unwrap_or(false) {
        return (asset_url(&g.cover_poster), "image".into());
    }
    (None, "image".into())
}

fn banner_urls(g: &Game) -> (Option<String>, String) {
    if g.banner_webm.as_ref().map(|p| Path::new(p).exists()).unwrap_or(false) {
        return (asset_url(&g.banner_webm), "video".into());
    }
    if let Some(c) = &g.banner {
        if Path::new(c).exists() {
            if is_motion(Path::new(c)) {
                if g.banner_poster.as_ref().map(|p| Path::new(p).exists()).unwrap_or(false) {
                    return (asset_url(&g.banner_poster), "image".into());
                }
                return (asset_url(&g.banner), "gif".into());
            }
            return (asset_url(&g.banner), "image".into());
        }
    }
    if g.banner_poster.as_ref().map(|p| Path::new(p).exists()).unwrap_or(false) {
        return (asset_url(&g.banner_poster), "image".into());
    }
    (None, "image".into())
}

pub fn serialize(state: &AppState, g: Game) -> GameDto {
    let (cover_url, cover_kind) = cover_urls(&g);
    let (banner_url, banner_kind) = banner_urls(&g);
    let icon_url = asset_url(&g.icon);
    let shots = state
        .db
        .list_screenshots(&g.id)
        .unwrap_or_default()
        .into_iter()
        .map(|path| ScreenshotDto {
            url: path.clone(),
            path,
        })
        .collect();
    let running = state.running.lock().contains_key(&g.id);
    let missing = !path_exists(&g.exe_path);
    GameDto {
        game: g,
        cover_url,
        cover_kind,
        banner_url,
        banner_kind,
        icon_url,
        screenshot_urls: shots,
        running,
        missing,
    }
}

pub fn serialize_all(state: &AppState) -> Result<Vec<GameDto>, String> {
    let games = state.db.list_games()?;
    Ok(games.into_iter().map(|g| serialize(state, g)).collect())
}

fn pretty_name(exe: &str) -> String {
    Path::new(exe)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Game")
        .replace(['_', '-', '.'], " ")
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn add_from_exe(state: &AppState, exe_path: String) -> Result<GameDto, String> {
    let exe_path = exe_path.trim().to_string();
    if exe_path.is_empty() || !Path::new(&exe_path).exists() {
        return Err("Executable not found".into());
    }
    let lower = exe_path.to_ascii_lowercase();
    for g in state.db.list_games()? {
        if g.exe_path
            .as_ref()
            .map(|p| p.to_ascii_lowercase() == lower)
            .unwrap_or(false)
        {
            return Ok(serialize(state, g));
        }
    }
    let id = Uuid::new_v4().to_string().replace('-', "");
    let id = id.chars().take(16).collect::<String>();
    let max_order = state
        .db
        .list_games()?
        .iter()
        .map(|g| g.sort_order)
        .max()
        .unwrap_or(-10);
    let cwd = Path::new(&exe_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string());
    let g = Game {
        id: id.clone(),
        name: pretty_name(&exe_path),
        exe_path: Some(exe_path),
        args: String::new(),
        cwd,
        cover: None,
        cover_webm: None,
        cover_poster: None,
        banner: None,
        banner_webm: None,
        banner_poster: None,
        icon: None,
        description: String::new(),
        category: "Games".into(),
        tags: vec![],
        favorite: false,
        accent: None,
        sort_order: max_order + 10,
        playtime_ms: 0,
        launch_count: 0,
        last_played: None,
        added_at: now_ms(),
        deleted_at: None,
    };
    state.db.upsert_game(&g)?;
    Ok(serialize(state, g))
}

pub fn update_game(state: &AppState, id: &str, patch: serde_json::Value) -> Result<GameDto, String> {
    let mut g = state
        .db
        .get_game(id)?
        .ok_or_else(|| "Game not found".to_string())?;
    let cur = serde_json::to_value(&g).map_err(|e| e.to_string())?;
    let mut map = cur.as_object().cloned().unwrap_or_default();
    if let Some(obj) = patch.as_object() {
        for (k, v) in obj {
            // ignore computed fields
            if matches!(
                k.as_str(),
                "coverUrl"
                    | "coverKind"
                    | "bannerUrl"
                    | "bannerKind"
                    | "iconUrl"
                    | "screenshotUrls"
                    | "running"
                    | "missing"
            ) {
                continue;
            }
            map.insert(k.clone(), v.clone());
        }
    }
    g = serde_json::from_value(serde_json::Value::Object(map)).map_err(|e| e.to_string())?;
    state.db.upsert_game(&g)?;
    Ok(serialize(state, g))
}

pub fn set_media_path(state: &AppState, id: &str, slot: &str, src: &str) -> Result<GameDto, String> {
    let src_path = PathBuf::from(src);
    if !src_path.exists() {
        return Err("File not found".into());
    }
    let (orig, webm, poster) = media::optimize_slot(&state.db, id, slot, &src_path)?;
    media::apply_optimized(&state.db, id, slot, orig, webm, poster)?;
    let g = state
        .db
        .get_game(id)?
        .ok_or_else(|| "Game not found".to_string())?;
    Ok(serialize(state, g))
}

#[cfg(windows)]
fn launch_windows(exe: &str, args: &str, cwd: Option<&str>, as_admin: bool) -> Result<u32, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const DETACHED_PROCESS: u32 = 0x00000008;

    if as_admin {
        // ShellExecute runas
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        fn wide(s: &str) -> Vec<u16> {
            OsStr::new(s).encode_wide().chain(Some(0)).collect()
        }
        let file = wide(exe);
        let params = wide(args);
        let dir = cwd.map(wide);
        let verb = wide("runas");
        let ret = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(verb.as_ptr()),
                PCWSTR(file.as_ptr()),
                PCWSTR(params.as_ptr()),
                dir.as_ref().map(|d| PCWSTR(d.as_ptr())).unwrap_or(PCWSTR::null()),
                SW_SHOWNORMAL,
            )
        };
        if (ret.0 as usize) <= 32 {
            return Err("Failed to elevate / launch".into());
        }
        return Ok(0);
    }

    let mut cmd = Command::new(exe);
    if !args.trim().is_empty() {
        // naive split — good enough for launch args
        for a in args.split_whitespace() {
            cmd.arg(a);
        }
    }
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    Ok(child.id())
}

#[cfg(not(windows))]
fn launch_windows(exe: &str, args: &str, cwd: Option<&str>, _as_admin: bool) -> Result<u32, String> {
    let mut cmd = Command::new(exe);
    for a in args.split_whitespace() {
        cmd.arg(a);
    }
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(child.id())
}

pub fn launch_game(app: &AppHandle, state: &AppState, id: &str) -> Result<GameDto, String> {
    let mut g = state
        .db
        .get_game(id)?
        .ok_or_else(|| "Game not found".to_string())?;
    let exe = g
        .exe_path
        .clone()
        .ok_or_else(|| "No executable".to_string())?;
    if !Path::new(&exe).exists() {
        return Err("Executable missing".into());
    }
    let settings = state.db.get_settings()?;
    let pid = launch_windows(
        &exe,
        &g.args,
        g.cwd.as_deref(),
        settings.launch_as_admin,
    )?;
    let started = now_ms();
    state.running.lock().insert(
        id.to_string(),
        RunningGame {
            started_at: started,
            pid,
        },
    );
    g.launch_count += 1;
    g.last_played = Some(started);
    state.db.upsert_game(&g)?;
    let _ = app.emit("library:changed", ());
    Ok(serialize(state, g))
}

pub fn recent_games(state: &AppState, limit: usize) -> Result<Vec<GameDto>, String> {
    let mut games = state.db.list_games()?;
    games.sort_by(|a, b| b.last_played.cmp(&a.last_played));
    Ok(games
        .into_iter()
        .take(limit)
        .map(|g| serialize(state, g))
        .collect())
}

pub fn island_games(state: &AppState) -> Result<Vec<GameDto>, String> {
    let mut recent = recent_games(state, 8)?;
    let favs: Vec<_> = state
        .db
        .list_games()?
        .into_iter()
        .filter(|g| g.favorite)
        .map(|g| serialize(state, g))
        .collect();
    // prefer favorites + recent unique
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for g in favs.into_iter().chain(recent.drain(..)) {
        if seen.insert(g.game.id.clone()) {
            out.push(g);
        }
        if out.len() >= 10 {
            break;
        }
    }
    Ok(out)
}

pub fn get_settings(state: &AppState) -> Result<Settings, String> {
    state.db.get_settings()
}

pub fn set_settings(state: &AppState, patch: serde_json::Value) -> Result<Settings, String> {
    state.db.set_settings(patch)
}
