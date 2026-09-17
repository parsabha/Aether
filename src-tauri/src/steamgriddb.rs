//! SteamGridDB API integration — search, browse, download, and auto-apply artwork.
//! API: https://www.steamgriddb.com/api/v2

use crate::games::{self, AppState};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const API_BASE: &str = "https://www.steamgriddb.com/api/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SgdbGame {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub release_date: Option<i64>,
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SgdbAsset {
    pub id: u64,
    pub url: String,
    pub thumb: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub style: String,
    #[serde(default)]
    pub score: Option<serde_json::Value>,
    #[serde(default)]
    pub mime: Option<String>,
}

#[derive(Deserialize)]
struct ApiList<T> {
    success: bool,
    data: Option<Vec<T>>,
    errors: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ApiOne<T> {
    success: bool,
    data: Option<T>,
    errors: Option<Vec<String>>,
}

fn api_key(_state: &AppState) -> Result<String, String> {
    let key = crate::sgdb_vault::unlock_bearer();
    if key.is_empty() {
        return Err("SteamGridDB is unavailable right now".into());
    }
    Ok(key)
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("Aether/1.3 (SteamGridDB collab)")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}

fn request_text(state: &AppState, path: &str) -> Result<String, String> {
    let key = api_key(state)?;
    let url = format!("{API_BASE}{path}");
    let resp = client()?
        .get(&url)
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .map_err(|e| format!("SteamGridDB request failed: {e}"))?;
    let status = resp.status();
    let body = resp.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("SteamGridDB HTTP {status}: {body}"));
    }
    Ok(body)
}

fn get_json_list<T: for<'de> Deserialize<'de>>(
    state: &AppState,
    path: &str,
) -> Result<Vec<T>, String> {
    let body = request_text(state, path)?;
    let parsed: ApiList<T> =
        serde_json::from_str(&body).map_err(|e| format!("SteamGridDB parse error: {e}"))?;
    if !parsed.success {
        let msg = parsed.errors.unwrap_or_default().join(", ");
        return Err(if msg.is_empty() {
            "SteamGridDB request failed".into()
        } else {
            msg
        });
    }
    Ok(parsed.data.unwrap_or_default())
}

fn get_json_one<T: for<'de> Deserialize<'de>>(
    state: &AppState,
    path: &str,
) -> Result<Option<T>, String> {
    let body = request_text(state, path)?;
    let parsed: ApiOne<T> =
        serde_json::from_str(&body).map_err(|e| format!("SteamGridDB parse error: {e}"))?;
    if !parsed.success {
        let msg = parsed.errors.unwrap_or_default().join(", ");
        return Err(if msg.is_empty() {
            "SteamGridDB request failed".into()
        } else {
            msg
        });
    }
    Ok(parsed.data)
}

fn slot_to_kind(slot: &str) -> Result<&'static str, String> {
    match slot {
        "cover" => Ok("grids"),
        "banner" => Ok("heroes"),
        "icon" => Ok("icons"),
        _ => Err("Unknown artwork slot".into()),
    }
}

fn preferred_dimensions(slot: &str) -> &'static str {
    match slot {
        // Vertical Steam capsule / grid used as cover
        "cover" => "600x900",
        // Wide hero / banner
        "banner" => "1920x620",
        "icon" => "",
        _ => "",
    }
}

fn asset_score(a: &SgdbAsset) -> f64 {
    match &a.score {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn pick_best(mut assets: Vec<SgdbAsset>) -> Option<SgdbAsset> {
    if assets.is_empty() {
        return None;
    }
    assets.sort_by(|a, b| {
        asset_score(b)
            .partial_cmp(&asset_score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    assets.into_iter().next()
}

pub fn search(state: &AppState, term: &str) -> Result<Vec<SgdbGame>, String> {
    let term = term.trim();
    if term.is_empty() {
        return Ok(vec![]);
    }
    let enc: String = url::form_urlencoded::byte_serialize(term.as_bytes()).collect();
    get_json_list(state, &format!("/search/autocomplete/{enc}"))
}

pub fn list_assets(
    state: &AppState,
    sgdb_game_id: u64,
    slot: &str,
) -> Result<Vec<SgdbAsset>, String> {
    let kind = slot_to_kind(slot)?;
    let dims = preferred_dimensions(slot);
    let path = if dims.is_empty() {
        format!("/{kind}/game/{sgdb_game_id}?types=static,animated")
    } else {
        format!("/{kind}/game/{sgdb_game_id}?dimensions={dims}&types=static,animated")
    };
    let mut assets = get_json_list::<SgdbAsset>(state, &path)?;
    if assets.is_empty() && !dims.is_empty() {
        assets = get_json_list(
            state,
            &format!("/{kind}/game/{sgdb_game_id}?types=static,animated"),
        )?;
    }
    assets.sort_by(|a, b| {
        asset_score(b)
            .partial_cmp(&asset_score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(assets)
}

fn list_assets_steam(
    state: &AppState,
    steam_app_id: u32,
    slot: &str,
) -> Result<Vec<SgdbAsset>, String> {
    let kind = slot_to_kind(slot)?;
    let dims = preferred_dimensions(slot);
    let path = if dims.is_empty() {
        format!("/{kind}/steam/{steam_app_id}?types=static,animated")
    } else {
        format!("/{kind}/steam/{steam_app_id}?dimensions={dims}&types=static,animated")
    };
    let mut assets = get_json_list::<SgdbAsset>(state, &path)?;
    if assets.is_empty() && !dims.is_empty() {
        assets = get_json_list(
            state,
            &format!("/{kind}/steam/{steam_app_id}?types=static,animated"),
        )?;
    }
    assets.sort_by(|a, b| {
        asset_score(b)
            .partial_cmp(&asset_score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(assets)
}

fn ext_from_url_or_mime(url: &str, mime: Option<&str>) -> String {
    if let Some(m) = mime {
        let m = m.to_ascii_lowercase();
        if m.contains("webp") {
            return "webp".into();
        }
        if m.contains("gif") {
            return "gif".into();
        }
        if m.contains("jpeg") || m.contains("jpg") {
            return "jpg".into();
        }
        if m.contains("png") {
            return "png".into();
        }
        if m.contains("webm") {
            return "webm".into();
        }
    }
    let lower = url.to_ascii_lowercase();
    for ext in ["png", "jpg", "jpeg", "webp", "gif", "webm", "ico"] {
        if lower.contains(&format!(".{ext}")) {
            return ext.to_string();
        }
    }
    "png".into()
}

fn download_to_tmp(state: &AppState, url: &str, mime: Option<&str>) -> Result<PathBuf, String> {
    let bytes = client()?
        .get(url)
        .send()
        .map_err(|e| format!("Download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Download failed: {e}"))?
        .bytes()
        .map_err(|e| e.to_string())?;

    let ext = ext_from_url_or_mime(url, mime);
    let tmp_dir = state.db.media.join("_sgdb_tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let path: PathBuf = tmp_dir.join(format!("sgdb-{}.{}", Uuid::new_v4(), ext));
    let mut f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    f.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn apply_asset(
    app: &AppHandle,
    game_id: &str,
    slot: &str,
    url: &str,
    mime: Option<&str>,
) -> Result<games::GameDto, String> {
    let _ = slot_to_kind(slot)?;
    let state = app.state::<AppState>();
    let path = download_to_tmp(&state, url, mime)?;
    let game = games::set_media_path(&state, game_id, slot, &path.to_string_lossy())?;
    let _ = std::fs::remove_file(&path);
    let _ = app.emit("library:changed", ());
    Ok(game)
}

fn apply_url(
    state: &AppState,
    game_id: &str,
    slot: &str,
    url: &str,
    mime: Option<&str>,
) -> Result<(), String> {
    let path = download_to_tmp(state, url, mime)?;
    let _ = games::set_media_path(state, game_id, slot, &path.to_string_lossy())?;
    let _ = std::fs::remove_file(&path);
    Ok(())
}

fn resolve_steam_app_id(_state: &AppState, game: &crate::db::Game) -> Option<u32> {
    let exe = game.exe_path.as_deref()?;
    let lower = exe.to_ascii_lowercase();
    if lower.starts_with("steam://") {
        return crate::steam::app_id_from_uri(exe);
    }
    crate::steam::resolve_steam_info(exe).map(|s| s.app_id)
}

fn slot_empty(game: &crate::db::Game, slot: &str) -> bool {
    match slot {
        "cover" => game.cover.as_ref().map(|s| s.is_empty()).unwrap_or(true),
        "banner" => game.banner.as_ref().map(|s| s.is_empty()).unwrap_or(true),
        "icon" => game.icon.as_ref().map(|s| s.is_empty()).unwrap_or(true),
        _ => true,
    }
}

/// Fill empty cover/banner/icon from the top-rated SteamGridDB assets.
/// Prefer Steam App ID when known; always fall back to searching by game name.
pub fn autofetch_artwork(state: &AppState, game_id: &str) -> Result<usize, String> {
    let _ = api_key(state)?;
    let game = state
        .db
        .get_game(game_id)?
        .ok_or_else(|| "Game not found".to_string())?;

    let needs_any = ["cover", "banner", "icon"]
        .iter()
        .any(|slot| slot_empty(&game, slot));
    if !needs_any {
        return Ok(0);
    }

    let steam_app_id = resolve_steam_app_id(state, &game);

    // Resolve SGDB game id: Steam mapping first, then name search.
    let mut sgdb_id = steam_app_id.and_then(|app_id| {
        get_json_one::<SgdbGame>(state, &format!("/games/steam/{app_id}"))
            .ok()
            .flatten()
            .map(|g| g.id)
    });
    if sgdb_id.is_none() {
        sgdb_id = search(state, &game.name)?
            .into_iter()
            .next()
            .map(|g| g.id);
    }
    let Some(sgdb_id) = sgdb_id else {
        return Err(format!("No SteamGridDB match for \"{}\"", game.name));
    };

    let mut applied = 0usize;
    for slot in ["cover", "banner", "icon"] {
        if !slot_empty(&game, slot) {
            continue;
        }

        let mut assets = if let Some(app_id) = steam_app_id {
            list_assets_steam(state, app_id, slot).unwrap_or_default()
        } else {
            Vec::new()
        };
        if assets.is_empty() {
            assets = list_assets(state, sgdb_id, slot).unwrap_or_default();
        }

        let Some(best) = pick_best(assets) else {
            continue;
        };
        if apply_url(state, game_id, slot, &best.url, best.mime.as_deref()).is_ok() {
            applied += 1;
        }
    }
    Ok(applied)
}
