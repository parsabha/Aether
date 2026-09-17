mod db;
mod game_detect;
mod game_perf;
mod games;
mod hotkey;
mod island;
mod island_feed;
mod media;
mod migrate;
mod overlay_stats;
mod session_telemetry;
mod steam;

use games::AppState;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{
    ipc::CapabilityBuilder,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;

/// Wait until the UI asset server accepts TCP on 127.0.0.1 (IPv4).
fn wait_for_ui_server(port: u16) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(100),
        )
        .is_ok()
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

fn spawn_ui_server(app: &AppHandle, port: u16) {
    let resolver = app.asset_resolver();
    std::thread::spawn(move || {
        let server = match tiny_http::Server::http(format!("127.0.0.1:{port}")) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Aether UI server failed to bind 127.0.0.1:{port}: {e}");
                return;
            }
        };
        for req in server.incoming_requests() {
            let raw = req.url().split(['?', '#']).next().unwrap_or("/");
            let key = if raw.is_empty() || raw == "/" {
                "/index.html".to_string()
            } else if raw.starts_with('/') {
                raw.to_string()
            } else {
                format!("/{raw}")
            };
            if let Some(asset) = resolver
                .get(key)
                .or_else(|| resolver.get("/index.html".into()))
            {
                let mut resp = tiny_http::Response::from_data(asset.bytes);
                if let Ok(h) =
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], asset.mime_type.as_bytes())
                {
                    resp.add_header(h);
                }
                if let Ok(h) = tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]) {
                    resp.add_header(h);
                }
                let _ = req.respond(resp);
            } else {
                let _ = req.respond(
                    tiny_http::Response::from_string("not found").with_status_code(404),
                );
            }
        }
    });
}

async fn off_ui<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_library(app: AppHandle) -> Result<Vec<games::GameDto>, String> {
    off_ui(move || {
        let state = app.state::<AppState>();
        games::serialize_all(&state)
    })
    .await
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<db::Settings, String> {
    games::get_settings(&state)
}

#[tauri::command]
fn list_play_sessions(
    state: tauri::State<'_, AppState>,
    game_id: String,
) -> Result<Vec<db::PlaySessionSummary>, String> {
    session_telemetry::list_sessions(&state.db, &game_id)
}

#[tauri::command]
fn get_play_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<Option<db::PlaySessionDetail>, String> {
    session_telemetry::get_session(&state.db, &session_id)
}

#[tauri::command]
fn delete_play_session(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<bool, String> {
    let game_id = state
        .db
        .get_play_session(&session_id)?
        .map(|d| d.summary.game_id);
    let ok = session_telemetry::delete_session(&state.db, &session_id)?;
    if ok {
        if let Some(gid) = game_id {
            let _ = app.emit("sessions:changed", &gid);
        }
    }
    Ok(ok)
}

#[tauri::command]
fn set_settings(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    patch: serde_json::Value,
) -> Result<db::Settings, String> {
    let prev = games::get_settings(&state)?;
    let s = games::set_settings(&state, patch)?;
    if s.launch_on_startup != prev.launch_on_startup {
        let handle = app.clone();
        let enabled = s.launch_on_startup;
        std::thread::spawn(move || sync_autostart(&handle, enabled));
    }
    // Never fail the settings write if the island window cannot be shown.
    let _ = island::show_island(&app, &state);
    if let Some(main) = app.get_webview_window("main") {
        island::apply_main_effects(&main, &s.blur);
    }
    sync_overlay(&app, &state, &s);
    let _ = hotkey::rebind_shortcuts(&app);
    let _ = app.emit("settings:changed", &s);
    Ok(s)
}

#[tauri::command]
async fn add_games(app: AppHandle, paths: Vec<String>) -> Result<Vec<games::GameDto>, String> {
    off_ui(move || {
        let state = app.state::<AppState>();
        let mut out = Vec::new();
        for p in paths {
            let lower = p.to_ascii_lowercase();
            if !(lower.ends_with(".exe")
                || lower.ends_with(".lnk")
                || lower.ends_with(".bat")
                || lower.ends_with(".cmd")
                || lower.ends_with(".url"))
            {
                continue;
            }
            out.push(games::add_from_exe(&state, p)?);
        }
        Ok(out)
    })
    .await
}

#[tauri::command]
fn update_game(
    state: tauri::State<'_, AppState>,
    id: String,
    patch: serde_json::Value,
) -> Result<games::GameDto, String> {
    games::update_game(&state, &id, patch)
}

#[tauri::command]
fn remove_game(state: tauri::State<'_, AppState>, id: String) -> Result<bool, String> {
    state.db.remove_game(&id)?;
    Ok(true)
}

#[tauri::command]
fn reorder_games(state: tauri::State<'_, AppState>, ordered_ids: Vec<String>) -> Result<bool, String> {
    state.db.reorder_games(&ordered_ids)?;
    Ok(true)
}

#[tauri::command]
async fn launch_game(app: AppHandle, id: String) -> Result<games::GameDto, String> {
    off_ui(move || {
        let state = app.state::<AppState>();
        games::launch_game(&app, &state, &id)
    })
    .await
}

#[tauri::command]
async fn recent_games(app: AppHandle) -> Result<Vec<games::GameDto>, String> {
    off_ui(move || {
        let state = app.state::<AppState>();
        games::recent_games(&state, 12)
    })
    .await
}

#[tauri::command]
async fn island_games(app: AppHandle) -> Result<Vec<games::GameDto>, String> {
    off_ui(move || {
        let state = app.state::<AppState>();
        games::island_games(&state)
    })
    .await
}

#[tauri::command]
async fn set_media_path(
    app: AppHandle,
    id: String,
    slot: String,
    src: String,
) -> Result<games::GameDto, String> {
    off_ui(move || {
        let state = app.state::<AppState>();
        games::set_media_path(&state, &id, &slot, &src)
    })
    .await
}

#[tauri::command]
fn remove_screenshot(
    state: tauri::State<'_, AppState>,
    id: String,
    path: String,
) -> Result<bool, String> {
    state.db.remove_screenshot(&id, &path)?;
    Ok(true)
}

#[tauri::command]
async fn optimize_library(app: AppHandle) -> Result<usize, String> {
    off_ui(move || {
        let state = app.state::<AppState>();
        media::reoptimize_library(&state.db)
    })
    .await
}

#[tauri::command]
async fn process_media_jobs(app: AppHandle) -> Result<usize, String> {
    off_ui(move || {
        let state = app.state::<AppState>();
        media::process_pending(&state.db)
    })
    .await
}

#[tauri::command]
fn toggle_island(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<bool, String> {
    island::toggle_island(&app, &state)
}

#[tauri::command]
fn island_layout(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    w: u32,
    h: u32,
) -> Result<(), String> {
    island::layout_island(&app, &state, w, h)
}

#[tauri::command]
fn show_main(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
    Ok(())
}

#[tauri::command]
fn get_displays(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let size = m.size();
            serde_json::json!({
                "id": i,
                "name": m.name().cloned().unwrap_or_else(|| format!("Display {}", i + 1)),
                "width": size.width,
                "height": size.height,
            })
        })
        .collect())
}

#[tauri::command]
async fn ffmpeg_available() -> bool {
    tauri::async_runtime::spawn_blocking(media::find_ffmpeg)
        .await
        .ok()
        .flatten()
        .is_some()
}

#[tauri::command]
async fn import_nebula(app: AppHandle, force: bool) -> Result<usize, String> {
    off_ui(move || {
        let state = app.state::<AppState>();
        migrate::import_nebula(&state.db, force)
    })
    .await
}

#[tauri::command]
fn open_data_dir() -> Result<(), String> {
    let dir = data_dir()?;
    let _ = open::that(&dir);
    Ok(())
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if p.is_file() {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("explorer")
                .arg("/select,")
                .arg(p)
                .spawn();
            return Ok(());
        }
    }
    if let Some(parent) = if p.is_dir() { Some(p) } else { p.parent() } {
        let _ = open::that(parent);
    }
    Ok(())
}

fn data_dir() -> Result<std::path::PathBuf, String> {
    let base = dirs::data_dir().ok_or_else(|| "No data dir".to_string())?;
    let dir = base.join("Aether");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Serve embedded UI over IPv4 loopback HTTP (WebView2 breaks on tauri.localhost;
    // binding the plugin to "localhost" often lands on ::1 while we navigate to 127.0.0.1).
    let port: u16 = 47823;
    island::UI_PORT.store(port, Ordering::Relaxed);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            let root = data_dir()?;
            let db = db::Db::open(&root)?;
            let imported = migrate::import_nebula_if_needed(&db).unwrap_or(0);
            if imported > 0 {
                println!("Imported {imported} games from Nebula");
            }
            match migrate::sync_nebula_customization(&db) {
                Ok(n) if n > 0 => println!("Synced {n} Nebula custom colors"),
                Err(e) => eprintln!("Nebula color sync failed: {e}"),
                _ => {}
            }

            // Allow CLI force re-import: Aether.exe --import-nebula
            let args: Vec<String> = std::env::args().collect();
            if args.iter().any(|a| a == "--import-nebula") {
                match migrate::import_nebula(&db, true) {
                    Ok(n) => println!("Force-imported {n} Nebula games"),
                    Err(e) => eprintln!("Nebula import failed: {e}"),
                }
            }

            let state = AppState {
                db,
                running: Mutex::new(HashMap::new()),
                notifications: Mutex::new(Vec::new()),
            };
            app.manage(state);
            island_feed::start_worker();
            spawn_ui_server(app.handle(), port);

            if !wait_for_ui_server(port) {
                eprintln!(
                    "Aether UI server did not start on 127.0.0.1:{port} — window may show connection refused"
                );
            }

            // Allow IPC from the loopback HTTP origin (required for custom commands).
            let remote = format!("http://127.0.0.1:{port}/*");
            app.handle().add_capability(
                CapabilityBuilder::new("localhost-ipc")
                    .remote(remote.clone())
                    .remote(format!("http://127.0.0.1:{port}"))
                    .windows(["main", "island"])
                    .permission("core:default")
                    .permission("allow-get-library")
                    .permission("allow-get-settings")
                    .permission("allow-set-settings")
                    .permission("allow-add-games")
                    .permission("allow-update-game")
                    .permission("allow-remove-game")
                    .permission("allow-reorder-games")
                    .permission("allow-launch-game")
                    .permission("allow-recent-games")
                    .permission("allow-island-games")
                    .permission("allow-island-feed")
                    .permission("allow-island-dismiss-notification")
                    .permission("allow-island-clear-notifications")
                    .permission("allow-island-mark-notifications-read")
                    .permission("allow-island-media-toggle")
                    .permission("allow-island-media-next")
                    .permission("allow-island-media-previous")
                    .permission("allow-set-media-path")
                    .permission("allow-remove-screenshot")
                    .permission("allow-optimize-library")
                    .permission("allow-process-media-jobs")
                    .permission("allow-toggle-island")
                    .permission("allow-island-layout")
                    .permission("allow-show-main")
                    .permission("allow-get-displays")
                    .permission("allow-ffmpeg-available")
                    .permission("allow-import-nebula")
                    .permission("allow-open-folder")
                    .permission("allow-open-data-dir")
                    .permission("core:window:allow-create")
                    .permission("core:webview:allow-create-webview-window")
                    .permission("core:window:allow-show")
                    .permission("core:window:allow-hide")
                    .permission("core:window:allow-close")
                    .permission("core:window:allow-minimize")
                    .permission("core:window:allow-maximize")
                    .permission("core:window:allow-unmaximize")
                    .permission("core:window:allow-set-focus")
                    .permission("core:window:allow-set-size")
                    .permission("core:window:allow-set-position")
                    .permission("core:window:allow-outer-position")
                    .permission("core:window:allow-outer-size")
                    .permission("core:window:allow-start-dragging")
                    .permission("core:window:allow-set-always-on-top")
                    .permission("core:window:allow-set-ignore-cursor-events")
                    .permission("core:window:allow-set-effects")
                    .permission("core:window:allow-set-background-color")
                    .permission("core:window:allow-is-maximized")
                    .permission("dialog:default")
                    .permission("fs:default")
                    .permission("shell:allow-open")
                    .permission("process:default")
                    .permission("opener:default")
                    .permission("autostart:allow-enable")
                    .permission("autostart:allow-disable")
                    .permission("autostart:allow-is-enabled")
                    .permission("global-shortcut:allow-register")
                    .permission("global-shortcut:allow-unregister")
                    .permission("global-shortcut:allow-is-registered"),
            )?;

            let main_url = format!("http://127.0.0.1:{port}/index.html")
                .parse()
                .map_err(|e: url::ParseError| e.to_string())?;
            let main = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(main_url))
                .title("Aether")
                .inner_size(1280.0, 800.0)
                .min_inner_size(960.0, 600.0)
                .decorations(false)
                // Must stay false: Win11 DWM acrylic/mica does not show through a layered window.
                .transparent(false)
                .shadow(true)
                .center()
                .background_color(tauri::window::Color(0, 0, 0, 0))
                .build()?;

            let blur = app
                .state::<AppState>()
                .db
                .get_settings()
                .map(|s| s.blur)
                .unwrap_or_else(|_| "acrylic".into());
            island::apply_main_effects(&main, &blur);

            // Create the island window up front (hidden if disabled) so the
            // settings toggle only has to show/hide it.
            let _ = island::ensure_island_window(app.handle());
            {
                let state = app.state::<AppState>();
                let _ = island::show_island(app.handle(), &state);
                if let Ok(s) = state.db.get_settings() {
                    sync_overlay(app.handle(), &state, &s);
                }
            }
            island::start_clickthrough_loop(app.handle().clone());

            // ffmpeg probe can stall on `where` / WinGet; never do it on the UI thread.
            std::thread::spawn(|| {
                let _ = media::find_ffmpeg();
            });

            // Hotkey
            let _ = hotkey::bind_shortcuts(app.handle());

            // Honor autostart + start minimized
            {
                let state = app.state::<AppState>();
                if let Ok(s) = state.db.get_settings() {
                    let handle = app.handle().clone();
                    let enabled = s.launch_on_startup;
                    std::thread::spawn(move || sync_autostart(&handle, enabled));
                    let args: Vec<String> = std::env::args().collect();
                    let minimized = s.start_minimized
                        && (args.iter().any(|a| a == "--minimized" || a == "--hidden")
                            || args.iter().any(|a| a.contains("autostart")));
                    if minimized {
                        let _ = main.hide();
                    }
                }
            }

            // Background: poll running + media jobs
            let handle2 = app.handle().clone();
            std::thread::spawn(move || loop {
                let gaming = game_perf::is_gaming();
                std::thread::sleep(std::time::Duration::from_secs(if gaming { 4 } else { 2 }));
                let Some(state) = handle2.try_state::<AppState>() else {
                    continue;
                };
                poll_running_inplace(&handle2, &state);
                // Don't chew disk/CPU on media converts while a game is up.
                if !game_perf::is_gaming() {
                    let _ = media::process_pending(&state.db);
                }
            });

            // Tray
            let show_i = MenuItem::with_id(app, "show", "Open Aether", true, None::<&str>)?;
            let island_i = MenuItem::with_id(app, "island", "Toggle Island", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &island_i, &quit_i])?;
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Aether")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        let _ = show_main(app.clone());
                    }
                    "island" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            let _ = island::toggle_island(app, &state);
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        let _ = show_main(app.clone());
                    }
                })
                .build(app)?;

            // Close main to tray instead of quit; re-apply backdrop after resize/maximize.
            if let Some(main) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                main.on_window_event(move |event| {
                    match event {
                        WindowEvent::CloseRequested { api, .. } => {
                            api.prevent_close();
                            if let Some(win) = handle.get_webview_window("main") {
                                let _ = win.hide();
                            }
                        }
                        WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                            static LAST_FX: AtomicU64 = AtomicU64::new(0);
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis() as u64)
                                .unwrap_or(0);
                            let prev = LAST_FX.load(Ordering::Relaxed);
                            if now.saturating_sub(prev) < 250 {
                                return;
                            }
                            LAST_FX.store(now, Ordering::Relaxed);
                            if let Some(win) = handle.get_webview_window("main") {
                                let blur = handle
                                    .try_state::<AppState>()
                                    .and_then(|s| s.db.get_settings().ok())
                                    .map(|s| s.blur)
                                    .unwrap_or_else(|| "acrylic".into());
                                island::apply_main_effects(&win, &blur);
                            }
                        }
                        _ => {}
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_library,
            get_settings,
            set_settings,
            add_games,
            update_game,
            remove_game,
            reorder_games,
            launch_game,
            recent_games,
            island_games,
            island_feed::island_feed,
            island_feed::island_dismiss_notification,
            island_feed::island_clear_notifications,
            island_feed::island_mark_notifications_read,
            island_feed::island_media_toggle,
            island_feed::island_media_next,
            island_feed::island_media_previous,
            set_media_path,
            remove_screenshot,
            optimize_library,
            process_media_jobs,
            toggle_island,
            island_layout,
            show_main,
            get_displays,
            ffmpeg_available,
            import_nebula,
            open_folder,
            open_data_dir,
            overlay_stats::overlay_stats,
            list_play_sessions,
            get_play_session,
            delete_play_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running Aether");
}

fn sync_overlay(app: &AppHandle, state: &AppState, settings: &db::Settings) {
    if settings.overlay_enabled {
        overlay_stats::ensure_started(app);
        let pid = game_detect::sync_detected_running(state).or_else(|| {
            state
                .running
                .lock()
                .values()
                .map(|rg| rg.pid)
                .find(|&p| p != 0)
        });
        let fps_pid = game_detect::detect_presentmon_pid(state).or(pid);
        overlay_stats::set_target_pid(fps_pid);
        let active = pid.is_some() || fps_pid.is_some();
        island::apply_hud_flags(active, settings.island_on_top);
        if let Some(island_win) = app.get_webview_window("island") {
            let on_top = active || settings.island_on_top;
            let _ = island_win.set_always_on_top(on_top);
            let _ = island_win.set_ignore_cursor_events(active);
            if active {
                // Settings/sync path: assert once. Periodic poll uses the throttled helper.
                island::reassert_topmost_now(&island_win);
                island::force_pass_through_hwnd(&island_win);
                let _ = island::show_island(app, state);
            }
        }
        if active {
            let _ = app.emit("library:changed", ());
        }
    } else {
        overlay_stats::stop();
        island::apply_hud_flags(false, settings.island_on_top);
        if let Some(island_win) = app.get_webview_window("island") {
            let _ = island_win.set_always_on_top(settings.island_on_top);
            let _ = island_win.set_ignore_cursor_events(false);
        }
    }
}

fn poll_running_inplace(app: &AppHandle, state: &AppState) {
    let mut finished = Vec::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    // Mark dead PIDs as waiting (pid=0) inside the handoff window so Steam
    // relaunches keep the same play session instead of ending it.
    {
        let mut running = state.running.lock();
        for rg in running.values_mut() {
            if rg.pid != 0 && !process_alive(rg.pid) {
                rg.pid = 0;
            }
        }
        running.retain(|id, rg| {
            if rg.pid == 0 {
                if now > rg.wait_until {
                    finished.push((id.clone(), rg.started_at, rg.session_id.clone()));
                    return false;
                }
                return true;
            }
            true
        });
    }

    if let Ok(settings) = state.db.get_settings() {
        let has_running = !state.running.lock().is_empty();
        // Always sample while a session is active (for session charts), even if HUD is off.
        if has_running || settings.overlay_enabled {
            overlay_stats::ensure_started(app);
            let prev_pid = overlay_stats::target_pid();
            // Single detection path — avoid a second EnumProcesses for PresentMon.
            // Also rebinds Steam handoffs: pid=0 → real game process.
            let pid = game_detect::sync_detected_running(state);
            overlay_stats::set_target_pid(pid);
            let active = pid.is_some() || has_running;
            game_perf::set_gaming(active);
            // Hide the main library window while gaming — WebView2/acrylic
            // compositing steals GPU time even when the game is focused.
            if let Some(main) = app.get_webview_window("main") {
                if active {
                    if main.is_visible().unwrap_or(false) {
                        let _ = main.hide();
                    }
                }
            }
            if settings.overlay_enabled {
                island::apply_hud_flags(active, settings.island_on_top);
                if let Some(win) = app.get_webview_window("island") {
                    let on_top = active || settings.island_on_top;
                    let _ = win.set_always_on_top(on_top);
                    let _ = win.set_ignore_cursor_events(active);
                    if active {
                        // Only reassert when the target process changes — never every poll.
                        if prev_pid != pid.unwrap_or(0) {
                            island::reassert_topmost_now(&win);
                            island::force_pass_through_hwnd(&win);
                        }
                    }
                }
                if prev_pid != pid.unwrap_or(0) {
                    let _ = app.emit("library:changed", ());
                }
            } else {
                island::apply_hud_flags(false, settings.island_on_top);
            }
        } else {
            overlay_stats::set_target_pid(None);
            game_perf::set_gaming(false);
        }
    }
    for (id, started, session_id) in finished {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let duration = (now - started).max(0);
        let _ = session_telemetry::end_session(&state.db, &session_id, now);
        if let Ok(Some(mut g)) = state.db.get_game(&id) {
            g.playtime_ms += duration;
            let name = g.name.clone();
            let _ = state.db.upsert_game(&g);
            island_feed::notify_session_end(app, state, &id, &name, duration);
        }
        let _ = app.emit("library:changed", ());
        let _ = app.emit("sessions:changed", &id);
    }
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };
    unsafe {
        let Ok(h) = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        ) else {
            return false;
        };
        let wait = WaitForSingleObject(h, 0);
        let _ = CloseHandle(h);
        wait.0 == 258u32 // WAIT_TIMEOUT => still running
    }
}

#[cfg(not(windows))]
fn process_alive(pid: u32) -> bool {
    pid != 0
}

#[cfg(windows)]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn sync_autostart(app: &AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt;
    #[cfg(windows)]
    {
        // HKCU Run cannot start an elevated app at logon. Prefer a
        // scheduled task with highest privileges so Aether comes up
        // as administrator with no extra UAC prompt.
        let _ = app.autolaunch().disable();
        if windows_startup_task(enabled).is_err() && enabled {
            let _ = app.autolaunch().enable();
        }
    }
    #[cfg(not(windows))]
    {
        if enabled {
            let _ = app.autolaunch().enable();
        } else {
            let _ = app.autolaunch().disable();
        }
    }
}

#[cfg(windows)]
fn write_utf16_le_bom(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    f.write_all(&[0xFF, 0xFE])?;
    for unit in text.encode_utf16() {
        f.write_all(&unit.to_le_bytes())?;
    }
    Ok(())
}

#[cfg(windows)]
fn windows_startup_task(enabled: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const TASK: &str = "Aether";

    let mut cmd = std::process::Command::new("schtasks");
    cmd.creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    if !enabled {
        let _ = cmd.args(["/Delete", "/TN", TASK, "/F"]).status();
        return Ok(());
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy();
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let name = std::env::var("USERNAME").unwrap_or_default();
    let user = if domain.is_empty() {
        name
    } else {
        format!("{domain}\\{name}")
    };
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Start Aether elevated at sign-in</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>false</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>--minimized</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        user = xml_escape(&user),
        exe = xml_escape(&exe_str),
    );

    let xml_path = std::env::temp_dir().join("aether-startup-task.xml");
    write_utf16_le_bom(&xml_path, &xml).map_err(|e| e.to_string())?;
    let xml_arg = xml_path.to_string_lossy().to_string();
    let ok = cmd
        .args(["/Create", "/TN", TASK, "/XML", &xml_arg, "/F"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::fs::remove_file(&xml_path);
    if ok {
        Ok(())
    } else {
        Err("Failed to register startup task".into())
    }
}

// Avoid unused open crate — use opener plugin / shell
mod open {
    use std::path::Path;
    pub fn that(path: &Path) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            std::process::Command::new("explorer").arg(path).spawn()?;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Ok(())
        }
    }
}
