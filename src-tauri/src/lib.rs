mod db;
mod games;
mod hotkey;
mod island;
mod island_feed;
mod media;
mod migrate;

use games::AppState;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::atomic::Ordering;
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

#[tauri::command]
fn get_library(state: tauri::State<'_, AppState>) -> Result<Vec<games::GameDto>, String> {
    games::serialize_all(&state)
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<db::Settings, String> {
    games::get_settings(&state)
}

#[tauri::command]
fn set_settings(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    patch: serde_json::Value,
) -> Result<db::Settings, String> {
    let s = games::set_settings(&state, patch)?;
    // Never fail the settings write if the island window cannot be shown.
    let _ = island::show_island(&app, &state);
    if let Some(main) = app.get_webview_window("main") {
        island::apply_main_effects(&main, &s.blur);
    }
    if let Some(island_win) = app.get_webview_window("island") {
        let _ = island_win.set_always_on_top(s.island_on_top);
    }
    let _ = hotkey::rebind_shortcuts(&app);
    let _ = app.emit("settings:changed", &s);
    Ok(s)
}

#[tauri::command]
fn add_games(state: tauri::State<'_, AppState>, paths: Vec<String>) -> Result<Vec<games::GameDto>, String> {
    let mut out = Vec::new();
    for p in paths {
        let lower = p.to_ascii_lowercase();
        if !(lower.ends_with(".exe")
            || lower.ends_with(".lnk")
            || lower.ends_with(".bat")
            || lower.ends_with(".cmd"))
        {
            continue;
        }
        out.push(games::add_from_exe(&state, p)?);
    }
    Ok(out)
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
fn launch_game(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<games::GameDto, String> {
    games::launch_game(&app, &state, &id)
}

#[tauri::command]
fn recent_games(state: tauri::State<'_, AppState>) -> Result<Vec<games::GameDto>, String> {
    games::recent_games(&state, 12)
}

#[tauri::command]
fn island_games(state: tauri::State<'_, AppState>) -> Result<Vec<games::GameDto>, String> {
    games::island_games(&state)
}

#[tauri::command]
fn set_media_path(
    state: tauri::State<'_, AppState>,
    id: String,
    slot: String,
    src: String,
) -> Result<games::GameDto, String> {
    games::set_media_path(&state, &id, &slot, &src)
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
fn optimize_library(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    media::reoptimize_library(&state.db)
}

#[tauri::command]
fn process_media_jobs(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    media::process_pending(&state.db)
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
fn ffmpeg_available() -> bool {
    media::find_ffmpeg().is_some()
}

#[tauri::command]
fn import_nebula(state: tauri::State<'_, AppState>, force: bool) -> Result<usize, String> {
    migrate::import_nebula(&state.db, force)
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
        .plugin(
            tauri_plugin_localhost::Builder::new(port)
                .host("127.0.0.1")
                .build(),
        )
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

            let main_url = format!("http://127.0.0.1:{port}/")
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
            // DWM often needs the HWND visible before the backdrop sticks.
            {
                let win = main.clone();
                let blur2 = blur.clone();
                std::thread::spawn(move || {
                    for ms in [80u64, 280, 700] {
                        std::thread::sleep(std::time::Duration::from_millis(ms));
                        island::apply_main_effects(&win, &blur2);
                    }
                });
            }

            // Create the island window up front (hidden if disabled) so the
            // settings toggle only has to show/hide it.
            let _ = island::ensure_island_window(app.handle());
            {
                let state = app.state::<AppState>();
                let _ = island::show_island(app.handle(), &state);
            }
            island::start_clickthrough_loop(app.handle().clone());

            // Resolve a working ffmpeg once (deletes broken lone copies)
            let _ = media::find_ffmpeg();

            // Hotkey
            let _ = hotkey::bind_shortcuts(app.handle());

            // Honor autostart + start minimized
            {
                let state = app.state::<AppState>();
                if let Ok(s) = state.db.get_settings() {
                    if s.launch_on_startup {
                        use tauri_plugin_autostart::ManagerExt;
                        let _ = app.autolaunch().enable();
                    }
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
                std::thread::sleep(std::time::Duration::from_secs(2));
                let Some(state) = handle2.try_state::<AppState>() else {
                    continue;
                };
                poll_running_inplace(&handle2, &state);
                let _ = media::process_pending(&state.db);
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
                        WindowEvent::Resized(_)
                        | WindowEvent::ScaleFactorChanged { .. }
                        | WindowEvent::Focused(_) => {
                            if let Some(win) = handle.get_webview_window("main") {
                                let blur = handle
                                    .state::<AppState>()
                                    .db
                                    .get_settings()
                                    .map(|s| s.blur)
                                    .unwrap_or_else(|_| "acrylic".into());
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
            open_data_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running Aether");
}

fn poll_running_inplace(app: &AppHandle, state: &AppState) {
    let mut finished = Vec::new();
    {
        let mut running = state.running.lock();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        running.retain(|id, rg| {
            if rg.pid == 0 {
                if now - rg.started_at > 120_000 {
                    finished.push((id.clone(), rg.started_at));
                    return false;
                }
                return true;
            }
            #[cfg(windows)]
            {
                use windows::Win32::Foundation::CloseHandle;
                use windows::Win32::System::Threading::{
                    OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
                    PROCESS_SYNCHRONIZE,
                };
                unsafe {
                    if let Ok(h) = OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                        false,
                        rg.pid,
                    ) {
                        let wait = WaitForSingleObject(h, 0);
                        let _ = CloseHandle(h);
                        if wait.0 == 258u32 {
                            return true;
                        }
                    }
                }
            }
            finished.push((id.clone(), rg.started_at));
            false
        });
    }
    for (id, started) in finished {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let duration = (now - started).max(0);
        if let Ok(Some(mut g)) = state.db.get_game(&id) {
            g.playtime_ms += duration;
            let name = g.name.clone();
            let _ = state.db.upsert_game(&g);
            island_feed::notify_session_end(app, state, &id, &name, duration);
        }
        let _ = app.emit("library:changed", ());
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
