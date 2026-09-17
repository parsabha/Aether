//! Steam library detection and launch helpers.
//!
//! Steam titles often refuse a direct `.exe` spawn (or exit immediately and
//! relaunch through the client). We resolve an AppID from the install tree and
//! start via `steam://rungameid/<id>` so DRM, overlay, and playtime tracking
//! behave like a normal Aether launch.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct SteamInfo {
    pub app_id: u32,
    pub name: Option<String>,
}

/// Best-effort Steam metadata for a game executable.
pub fn resolve_steam_info(exe_path: &str) -> Option<SteamInfo> {
    let path = Path::new(exe_path);
    if !path.exists() {
        return None;
    }

    if let Some(info) = from_steam_appid_txt(path) {
        return Some(info);
    }
    if let Some(info) = from_steamapps_tree(path) {
        return Some(info);
    }
    // steam_api*.dll nearby is a strong signal, but without an AppID we cannot
    // use steam:// — still treat as Steam for handoff grace in the poller.
    None
}

/// True when the exe lives under a Steam library or ships Steamworks.
pub fn looks_like_steam_game(exe_path: &str) -> bool {
    if resolve_steam_info(exe_path).is_some() {
        return true;
    }
    let path = Path::new(exe_path);
    if steamapps_common_root(path).is_some() {
        return true;
    }
    has_steamworks_dll(path)
}

pub fn launch_steam_app(app_id: u32) -> Result<(), String> {
    let uri = format!("steam://rungameid/{app_id}");
    #[cfg(windows)]
    {
        crate::games::shell_execute_uri(&uri)?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(&uri)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn from_steam_appid_txt(exe: &Path) -> Option<SteamInfo> {
    // Walk a few parents — some titles nest the exe under bin/win64/ etc.
    let mut dir = exe.parent()?;
    for _ in 0..6 {
        let candidate = dir.join("steam_appid.txt");
        if candidate.is_file() {
            let raw = fs::read_to_string(&candidate).ok()?;
            let id = parse_app_id(raw.lines().next().unwrap_or(&raw).trim())?;
            if let Some((steamapps, _)) = steamapps_common_root(exe) {
                let name = lookup_manifest_for_installdir(&steamapps, id).and_then(|m| m.name);
                return Some(SteamInfo { app_id: id, name });
            }
            return Some(SteamInfo {
                app_id: id,
                name: None,
            });
        }
        dir = dir.parent()?;
    }
    None
}

fn from_steamapps_tree(exe: &Path) -> Option<SteamInfo> {
    let (steamapps, install_dir) = steamapps_common_root(exe)?;
    let manifests = load_manifests(&steamapps);
    let key = install_dir.to_ascii_lowercase();
    manifests.into_iter().find_map(|m| {
        let matches = m
            .installdir
            .as_ref()
            .map(|d| d.to_ascii_lowercase() == key)
            .unwrap_or(false);
        if matches {
            Some(SteamInfo {
                app_id: m.app_id,
                name: m.name,
            })
        } else {
            None
        }
    })
}

fn steamapps_common_root(exe: &Path) -> Option<(PathBuf, String)> {
    let mut cur = exe.parent()?;
    loop {
        let name = cur.file_name()?.to_string_lossy();
        if name.eq_ignore_ascii_case("common") {
            let steamapps = cur.parent()?;
            let parent_name = steamapps.file_name()?.to_string_lossy();
            if parent_name.eq_ignore_ascii_case("steamapps") {
                // install folder is the child of common that contains the exe
                let install = exe
                    .strip_prefix(cur)
                    .ok()?
                    .components()
                    .next()?
                    .as_os_str()
                    .to_string_lossy()
                    .to_string();
                return Some((steamapps.to_path_buf(), install));
            }
        }
        cur = cur.parent()?;
    }
}

fn has_steamworks_dll(exe: &Path) -> bool {
    let dir = match exe.parent() {
        Some(d) => d,
        None => return false,
    };
    for name in ["steam_api64.dll", "steam_api.dll", "steamclient64.dll"] {
        if dir.join(name).is_file() {
            return true;
        }
    }
    // Also check one level up (Unity/Unreal layouts).
    if let Some(parent) = dir.parent() {
        for name in ["steam_api64.dll", "steam_api.dll"] {
            if parent.join(name).is_file() {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Clone)]
struct Manifest {
    app_id: u32,
    name: Option<String>,
    installdir: Option<String>,
}

fn lookup_manifest_for_installdir(steamapps: &Path, app_id: u32) -> Option<Manifest> {
    load_manifests(steamapps)
        .into_iter()
        .find(|m| m.app_id == app_id)
}

fn load_manifests(steamapps: &Path) -> Vec<Manifest> {
    let Ok(entries) = fs::read_dir(steamapps) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(entry.path()) {
            if let Some(m) = parse_acf_manifest(&text) {
                out.push(m);
            }
        }
    }
    out
}

fn parse_acf_manifest(text: &str) -> Option<Manifest> {
    let app_id = vdf_string(text, "appid").and_then(|s| parse_app_id(&s))?;
    let name = vdf_string(text, "name");
    let installdir = vdf_string(text, "installdir");
    Some(Manifest {
        app_id,
        name,
        installdir,
    })
}

/// Minimal VDF string lookup: `"key"\t\t"value"`.
fn vdf_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let lower = text.to_ascii_lowercase();
    let needle_l = needle.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find(&needle_l) {
        let start = search_from + rel + needle.len();
        let rest = &text[start..];
        let mut chars = rest.chars().peekable();
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        if chars.next() != Some('"') {
            search_from = start;
            continue;
        }
        let mut value = String::new();
        for c in chars {
            if c == '"' {
                return Some(value);
            }
            value.push(c);
        }
        break;
    }
    None
}

fn parse_app_id(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.parse::<u32>().ok().filter(|&id| id > 0)
}

/// Registry / default-path Steam install (used if we ever need steam.exe).
#[allow(dead_code)]
pub fn steam_install_path() -> Option<PathBuf> {
    static CACHED: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            #[cfg(windows)]
            {
                if let Some(p) = steam_path_from_registry() {
                    return Some(p);
                }
            }
            let home = dirs::home_dir()?;
            for rel in [
                r"AppData\Local\Steam",
                r"Program Files (x86)\Steam",
                r"Program Files\Steam",
            ] {
                let p = if rel.starts_with("AppData") {
                    home.join(rel)
                } else {
                    PathBuf::from(format!(r"C:\{rel}"))
                };
                if p.join("steam.exe").is_file() {
                    return Some(p);
                }
            }
            None
        })
        .clone()
}

#[cfg(windows)]
fn steam_path_from_registry() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Valve\Steam",
            "/v",
            "SteamPath",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(idx) = line.to_ascii_lowercase().find("steampath") {
            let rest = line[idx + "steampath".len()..].trim();
            // REG_SZ    C:\Program Files (x86)\Steam
            let parts: Vec<_> = rest.split_whitespace().collect();
            if parts.len() >= 2 {
                let path = parts[1..].join(" ");
                let pb = PathBuf::from(path.replace('/', "\\"));
                if pb.join("steam.exe").is_file() {
                    return Some(pb);
                }
            }
        }
    }
    None
}

/// Parse `steam://rungameid/123` (or similar) from a Windows `.url` shortcut.
pub fn parse_steam_url_file(path: &str) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("url=") {
            return parse_steam_uri(rest);
        }
        if lower.starts_with("steam://") {
            return parse_steam_uri(&lower);
        }
    }
    None
}

fn parse_steam_uri(uri: &str) -> Option<u32> {
    let lower = uri.trim().to_ascii_lowercase();
    for prefix in ["steam://rungameid/", "steam://run/"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let id = rest.split(&['/', '?', '&', '#'][..]).next()?;
            return parse_app_id(id);
        }
    }
    None
}

/// Resolve an AppID from a `steam://…` URI string.
pub fn app_id_from_uri(uri: &str) -> Option<u32> {
    parse_steam_uri(uri)
}

/// Map AppID → installdir across all known Steam libraries (libraryfolders.vdf).
#[allow(dead_code)]
fn all_library_manifests() -> HashMap<u32, Manifest> {
    let mut map = HashMap::new();
    let Some(root) = steam_install_path() else {
        return map;
    };
    let steamapps = root.join("steamapps");
    for m in load_manifests(&steamapps) {
        map.insert(m.app_id, m);
    }
    let vdf = steamapps.join("libraryfolders.vdf");
    if let Ok(text) = fs::read_to_string(vdf) {
        for path in vdf_library_paths(&text) {
            let lib_apps = PathBuf::from(path).join("steamapps");
            for m in load_manifests(&lib_apps) {
                map.insert(m.app_id, m);
            }
        }
    }
    map
}

fn vdf_library_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = text.to_ascii_lowercase();
    let mut search = 0;
    while let Some(rel) = lower[search..].find("\"path\"") {
        let start = search + rel + "\"path\"".len();
        let rest = &text[start..];
        let mut chars = rest.chars().peekable();
        while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
            chars.next();
        }
        if chars.next() != Some('"') {
            search = start;
            continue;
        }
        let mut value = String::new();
        for c in chars {
            if c == '"' {
                break;
            }
            value.push(c);
        }
        let normalized = value.replace("\\\\", "\\");
        if !normalized.is_empty() {
            out.push(normalized);
        }
        search = start;
    }
    out
}
