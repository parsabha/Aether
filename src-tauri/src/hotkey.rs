use crate::games::AppState;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

fn shortcut_from_settings(key: &str) -> Shortcut {
    match key.to_uppercase().as_str() {
        "F8" => Shortcut::new(None, Code::F8),
        "F10" => Shortcut::new(None, Code::F10),
        "F11" => Shortcut::new(None, Code::F11),
        "F12" => Shortcut::new(None, Code::F12),
        "CTRL+SHIFT+S" => Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS),
        _ => Shortcut::new(None, Code::F9),
    }
}

pub fn capture_for_running(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let settings = state.db.get_settings()?;
    if !settings.screenshot_hotkey_enabled {
        return Ok(());
    }
    let running: Vec<String> = state.running.lock().keys().cloned().collect();
    let target = if let Some(id) = running.first() {
        id.clone()
    } else {
        let recent = crate::games::recent_games(state, 1)?;
        recent
            .first()
            .map(|g| g.game.id.clone())
            .ok_or_else(|| "No running or recent game".to_string())?
    };

    let dir = state.db.game_media_dir(&target).join("screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("shot-{}.png", chrono::Utc::now().timestamp_millis()));

    #[cfg(windows)]
    {
        use xcap::Monitor;
        let monitors = Monitor::all().map_err(|e| e.to_string())?;
        let mon = monitors
            .into_iter()
            .next()
            .ok_or_else(|| "No monitor".to_string())?;
        let img = mon.capture_image().map_err(|e| e.to_string())?;
        img.save(&path).map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))]
    {
        return Err("Screenshots only on Windows".into());
    }

    state
        .db
        .add_screenshot(&target, &path.to_string_lossy())?;
    let game_name = state
        .db
        .get_game(&target)
        .ok()
        .flatten()
        .map(|g| g.name)
        .unwrap_or_else(|| "Game".into());
    crate::island_feed::notify_screenshot(app, state, &target, &game_name);
    let _ = app.emit(
        "screenshot:captured",
        serde_json::json!({ "gameId": target, "path": path }),
    );
    let _ = app.emit("library:changed", ());
    Ok(())
}

pub fn rebind_shortcuts(app: &AppHandle) -> Result<(), String> {
    let _ = app.global_shortcut().unregister_all();
    let Some(state) = app.try_state::<AppState>() else {
        return Ok(());
    };
    let settings = state.db.get_settings()?;
    if !settings.screenshot_hotkey_enabled {
        return Ok(());
    }
    let shortcut = shortcut_from_settings(&settings.screenshot_hotkey);
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            if let Some(state) = handle.try_state::<AppState>() {
                let _ = capture_for_running(&handle, state.inner());
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn bind_shortcuts(app: &AppHandle) -> Result<(), String> {
    rebind_shortcuts(app)
}
