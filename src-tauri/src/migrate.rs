use crate::db::{Db, Game};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub fn nebula_dir() -> Option<PathBuf> {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(&appdata).join("Nebula");
        if p.join("library.json").exists() {
            return Some(p);
        }
    }
    let data = dirs::data_dir()?;
    let p = data.join("Nebula");
    if p.join("library.json").exists() {
        return Some(p);
    }
    None
}

fn json_str(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

fn json_i64(v: &Value, keys: &[&str]) -> Option<i64> {
    for k in keys {
        if let Some(n) = v.get(*k).and_then(|x| x.as_i64()) {
            return Some(n);
        }
        if let Some(n) = v.get(*k).and_then(|x| x.as_f64()) {
            return Some(n as i64);
        }
    }
    None
}

fn json_bool(v: &Value, keys: &[&str], default: bool) -> bool {
    for k in keys {
        if let Some(b) = v.get(*k).and_then(|x| x.as_bool()) {
            return b;
        }
    }
    default
}

/// Import Nebula library. If `force`, upsert over existing Aether rows with same id.
pub fn import_nebula(db: &Db, force: bool) -> Result<usize, String> {
    let Some(nebula) = nebula_dir() else {
        return Err("Nebula library not found in %APPDATA%\\Nebula".into());
    };
    let raw = fs::read_to_string(nebula.join("library.json")).map_err(|e| e.to_string())?;
    let parsed: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let games = parsed
        .get("games")
        .and_then(|g| g.as_array())
        .cloned()
        .unwrap_or_default();

    let mut imported = 0;
    for g in games {
        let id = g
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        if !force && db.get_game(&id)?.is_some() {
            continue;
        }

        let name = json_str(&g, &["name"]).unwrap_or_else(|| "Game".into());
        let exe_path = json_str(&g, &["exePath", "exe_path"]);
        let args = json_str(&g, &["args"]).unwrap_or_default();
        let cwd = json_str(&g, &["cwd"]);
        let description = json_str(&g, &["description"]).unwrap_or_default();
        let category = json_str(&g, &["category"]).unwrap_or_else(|| "Games".into());
        let tags = g
            .get("tags")
            .cloned()
            .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
            .unwrap_or_default();
        let favorite = json_bool(&g, &["favorite"], false);
        let accent = json_str(&g, &["accentColor", "accent"]);
        let sort_order = json_i64(&g, &["sortOrder", "sort_order"]).unwrap_or(0);
        // Nebula stores seconds; Aether stores ms
        let playtime_ms = json_i64(&g, &["playtimeMs", "playtime_ms"]).unwrap_or_else(|| {
            json_i64(&g, &["playtimeSeconds", "playtime_seconds"]).unwrap_or(0) * 1000
        });
        let launch_count = json_i64(&g, &["launchCount", "launch_count"]).unwrap_or(0);
        let last_played = json_i64(&g, &["lastPlayed", "last_played"]).and_then(|v| {
            if v == 0 {
                None
            } else {
                Some(v)
            }
        });
        let added_at =
            json_i64(&g, &["addedAt", "added_at"]).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        let dest = db.game_media_dir(&id);
        let cover = copy_media(g.get("cover").and_then(|v| v.as_str()), &dest, "cover");
        let banner = copy_media(g.get("banner").and_then(|v| v.as_str()), &dest, "banner");
        let icon = copy_media(g.get("icon").and_then(|v| v.as_str()), &dest, "icon");

        let existing = db.get_game(&id)?;
        let game = Game {
            id: id.clone(),
            name,
            exe_path,
            args,
            cwd,
            cover: cover.clone().or_else(|| existing.as_ref().and_then(|e| e.cover.clone())),
            cover_webm: existing.as_ref().and_then(|e| e.cover_webm.clone()),
            cover_poster: existing.as_ref().and_then(|e| e.cover_poster.clone()),
            banner: banner.clone().or_else(|| existing.as_ref().and_then(|e| e.banner.clone())),
            banner_webm: existing.as_ref().and_then(|e| e.banner_webm.clone()),
            banner_poster: existing.as_ref().and_then(|e| e.banner_poster.clone()),
            icon: icon.or_else(|| existing.as_ref().and_then(|e| e.icon.clone())),
            description,
            category,
            tags,
            favorite,
            accent,
            sort_order,
            playtime_ms,
            launch_count,
            last_played,
            added_at,
            deleted_at: None,
        };
        db.upsert_game(&game)?;

        if let Some(arr) = g.get("screenshots").and_then(|v| v.as_array()) {
            let existing_shots = db.list_screenshots(&id).unwrap_or_default();
            for (i, s) in arr.iter().enumerate() {
                if let Some(p) = s.as_str() {
                    if let Some(copied) = copy_media(Some(p), &dest, &format!("shot-{i}")) {
                        if !existing_shots.iter().any(|e| e == &copied) {
                            let _ = db.add_screenshot(&id, &copied);
                        }
                    }
                }
            }
        }

        for (slot, src) in [("cover", cover), ("banner", banner)] {
            if let Some(s) = src {
                let path = PathBuf::from(&s);
                if path.exists() {
                    // Queue for background transcode so import finishes quickly
                    let _ = db.enqueue_media_job(&id, slot, &s);
                    let _ = path;
                }
            }
        }

        imported += 1;
    }

    // Settings from Nebula
    if let Some(s) = parsed.get("settings") {
        let mut patch = serde_json::Map::new();
        if let Some(a) = s.get("accent") {
            patch.insert("accent".into(), a.clone());
        }
        if let Some(v) = s.get("view") {
            patch.insert("view".into(), v.clone());
        }
        if let Some(v) = s.get("sort") {
            patch.insert("sort".into(), v.clone());
        }
        if let Some(v) = s.get("launchAsAdmin") {
            patch.insert("launchAsAdmin".into(), v.clone());
        }
        if let Some(v) = s.get("launchOnStartup") {
            patch.insert("launchOnStartup".into(), v.clone());
        }
        if let Some(v) = s.get("startMinimized") {
            patch.insert("startMinimized".into(), v.clone());
        }
        if let Some(v) = s.get("reduceMotion") {
            patch.insert("reduceMotion".into(), v.clone());
        }
        if let Some(v) = s.get("blur") {
            let blur = v.as_str().unwrap_or("acrylic");
            patch.insert(
                "blur".into(),
                Value::String(if blur == "none" {
                    "solid".into()
                } else {
                    blur.into()
                }),
            );
        }
        // Do not copy Nebula widget on/off — Aether's island is independent.
        if let Some(v) = s.get("widgetOnTop") {
            patch.insert("islandOnTop".into(), v.clone());
        }
        if let Some(v) = s.get("widgetDisplay") {
            patch.insert("islandDisplay".into(), v.clone());
        }
        if let Some(v) = s.get("screenshotHotkeyEnabled") {
            patch.insert("screenshotHotkeyEnabled".into(), v.clone());
        }
        if let Some(v) = s.get("screenshotHotkey") {
            patch.insert("screenshotHotkey".into(), v.clone());
        }
        patch.insert("nebulaImported".into(), Value::Bool(true));
        let _ = db.set_settings(Value::Object(patch));
    } else {
        let _ = db.set_settings(serde_json::json!({ "nebulaImported": true }));
    }

    Ok(imported)
}

/// Copy per-game accent (and empty descriptions) from Nebula into existing Aether rows.
pub fn sync_nebula_customization(db: &Db) -> Result<usize, String> {
    let Some(nebula) = nebula_dir() else {
        return Ok(0);
    };
    let raw = fs::read_to_string(nebula.join("library.json")).map_err(|e| e.to_string())?;
    let parsed: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let games = parsed
        .get("games")
        .and_then(|g| g.as_array())
        .cloned()
        .unwrap_or_default();
    let mut updated = 0;
    for g in games {
        let id = g.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let Some(mut existing) = db.get_game(id)? else {
            continue;
        };
        let mut changed = false;
        if let Some(accent) = json_str(&g, &["accentColor", "accent"]) {
            if existing.accent.as_deref() != Some(accent.as_str()) {
                existing.accent = Some(accent);
                changed = true;
            }
        }
        if existing.description.is_empty() {
            if let Some(d) = json_str(&g, &["description"]) {
                if !d.is_empty() {
                    existing.description = d;
                    changed = true;
                }
            }
        }
        if existing.tags.is_empty() {
            let tags = g
                .get("tags")
                .cloned()
                .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
                .unwrap_or_default();
            if !tags.is_empty() {
                existing.tags = tags;
                changed = true;
            }
        }
        if changed {
            db.upsert_game(&existing)?;
            updated += 1;
        }
    }
    Ok(updated)
}

pub fn import_nebula_if_needed(db: &Db) -> Result<usize, String> {
    let settings = db.get_settings()?;
    if settings.nebula_imported {
        return Ok(0);
    }
    match import_nebula(db, false) {
        Ok(n) => Ok(n),
        Err(_) => {
            let _ = db.set_settings(serde_json::json!({ "nebulaImported": true }));
            Ok(0)
        }
    }
}

fn copy_media(src: Option<&str>, dest_dir: &Path, stem: &str) -> Option<String> {
    let src = src?;
    let src_path = Path::new(src);
    if !src_path.exists() {
        return None;
    }
    let ext = src_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let dest = dest_dir.join(format!("{stem}.{ext}"));
    if dest.exists() {
        return Some(dest.to_string_lossy().to_string());
    }
    if fs::copy(src_path, &dest).is_ok() {
        Some(dest.to_string_lossy().to_string())
    } else {
        Some(src.to_string())
    }
}
