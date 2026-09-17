use crate::games::AppState;
use crate::overlay_stats;
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, Ordering};
use tauri::{
    window::Color,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub static UI_PORT: AtomicU16 = AtomicU16::new(47823);
static PILL_W: AtomicU32 = AtomicU32::new(196);
static PILL_H: AtomicU32 = AtomicU32::new(38);
/// HWND_TOPMOST while in-game HUD is active (or user opted into island_on_top).
static FORCE_TOPMOST: AtomicU32 = AtomicU32::new(0);
/// Full mouse pass-through — display-only HUD over a running game.
static PASS_THROUGH: AtomicU32 = AtomicU32::new(0);

/// Transparent host that the pill morphs inside. Never resize this — CSS animates the pill.
const HOST_W: u32 = 560;
const HOST_H: u32 = 560;
const HOST_TOP: i32 = 8;

pub fn ui_base() -> String {
    format!("http://127.0.0.1:{}", UI_PORT.load(Ordering::Relaxed))
}

pub fn ensure_island_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(win) = app.get_webview_window("island") {
        return Ok(win);
    }
    let url = format!("{}/index.html#island", ui_base())
        .parse()
        .map_err(|e: url::ParseError| e.to_string())?;
    WebviewWindowBuilder::new(app, "island", WebviewUrl::External(url))
        .title("Aether Island")
        .inner_size(HOST_W as f64, HOST_H as f64)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .background_color(Color(0, 0, 0, 0))
        .build()
        .map_err(|e| e.to_string())
}

pub fn position_island(app: &AppHandle, state: &AppState) -> Result<(), String> {
    layout_island(app, state, 208, 40)
}

pub fn layout_island(
    app: &AppHandle,
    state: &AppState,
    pill_w: u32,
    pill_h: u32,
) -> Result<(), String> {
    let win = ensure_island_window(app)?;
    let settings = state.db.get_settings()?;
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    if monitors.is_empty() {
        return Ok(());
    }
    let monitor = if let Some(idx) = settings.island_display {
        monitors
            .get(idx as usize)
            .cloned()
            .unwrap_or_else(|| monitors[0].clone())
    } else {
        app.primary_monitor()
            .ok()
            .flatten()
            .unwrap_or_else(|| monitors[0].clone())
    };
    let size = monitor.size();
    let pos = monitor.position();
    let (pw, ph) = (pill_w.clamp(120, HOST_W - 20), pill_h.clamp(30, HOST_H - 20));
    PILL_W.store(pw, Ordering::Relaxed);
    PILL_H.store(ph, Ordering::Relaxed);

    let game_running = state.running.lock().values().any(|rg| rg.pid != 0)
        || overlay_stats::target_pid() != 0;
    let in_game_hud = settings.overlay_enabled && game_running;
    apply_hud_flags(in_game_hud, settings.island_on_top);

    // In-game HUD: shrink the transparent host to the pill so we don't leave a
    // giant invisible hit-box over half the screen if click-through glitches.
    let (host_w, host_h) = if in_game_hud {
        let w = (pw + 28).clamp(200, HOST_W);
        let h = (ph + 20).clamp(48, 72);
        (w, h)
    } else {
        (HOST_W, HOST_H)
    };
    let x = pos.x + (size.width as i32 - host_w as i32) / 2;
    let y = pos.y + HOST_TOP;

    // Skip SetWindowPos when geometry is unchanged — each call hitchs DWM/Vulkan.
    static LAST_WH: AtomicU64 = AtomicU64::new(0);
    static LAST_XY: AtomicU64 = AtomicU64::new(0);
    let wh = ((host_w as u64) << 32) | (host_h as u64);
    let xy = ((x as u32 as u64) << 32) | (y as u32 as u64);
    let geom_changed =
        LAST_WH.swap(wh, Ordering::Relaxed) != wh || LAST_XY.swap(xy, Ordering::Relaxed) != xy;
    if geom_changed {
        win.set_size(PhysicalSize::new(host_w, host_h))
            .map_err(|e| e.to_string())?;
        win.set_position(PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }

    let on_top = in_game_hud || settings.island_on_top;
    let _ = win.set_always_on_top(on_top);
    let _ = win.set_ignore_cursor_events(in_game_hud);
    // Never reassert TOPMOST on every layout tick while HUD is up.
    if on_top && (!in_game_hud || geom_changed) {
        reassert_topmost(&win);
    }
    if in_game_hud && geom_changed {
        force_pass_through_hwnd(&win);
    }
    Ok(())
}

/// In-game overlay HUD: topmost + full click-through. Desktop island uses island_on_top.
pub fn apply_hud_flags(in_game_hud: bool, island_on_top: bool) {
    FORCE_TOPMOST.store(u32::from(in_game_hud || island_on_top), Ordering::Relaxed);
    PASS_THROUGH.store(u32::from(in_game_hud), Ordering::Relaxed);
}

static LAST_TOPMOST_REASSERT: AtomicU64 = AtomicU64::new(0);

/// Reassert HWND_TOPMOST, but at most ~once every 8s while already active.
/// Calling SetWindowPos every poll (2s) hitchs DWM-composed Vulkan games.
pub fn reassert_topmost(win: &WebviewWindow) {
    reassert_topmost_throttled(win, false);
}

pub fn reassert_topmost_now(win: &WebviewWindow) {
    reassert_topmost_throttled(win, true);
}

fn reassert_topmost_throttled(win: &WebviewWindow, force: bool) {
    #[cfg(windows)]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let prev = LAST_TOPMOST_REASSERT.load(Ordering::Relaxed);
        if !force && now.saturating_sub(prev) < 15_000 {
            return;
        }
        LAST_TOPMOST_REASSERT.store(now, Ordering::Relaxed);
        if let Ok(native) = win.hwnd() {
            let hwnd = HWND(native.0 as _);
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (win, force);
    }
}

/// WebView2 often ignores parent WS_EX_TRANSPARENT; combine styles + Tauri ignore.
pub fn force_pass_through_hwnd(win: &WebviewWindow) {
    let _ = win.set_ignore_cursor_events(true);
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_TRANSPARENT,
        };
        if let Ok(native) = win.hwnd() {
            let hwnd = HWND(native.0 as _);
            unsafe {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let next = ex
                    | (WS_EX_TRANSPARENT.0 as isize)
                    | (WS_EX_LAYERED.0 as isize);
                // Only touch styles when needed — SetWindowPos/FRAMECHANGED every poll
                // hitchs composed Vulkan games (RDR2) without changing FPS.
                if next != ex {
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = win;
    }
}

pub fn show_island(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let settings = state.db.get_settings()?;
    if !settings.island_enabled {
        if let Some(win) = app.get_webview_window("island") {
            let _ = win.hide();
        }
        return Ok(());
    }
    let _ = ensure_island_window(app)?;
    position_island(app, state)?;
    if let Some(win) = app.get_webview_window("island") {
        let _ = win.show();
    }
    Ok(())
}

pub fn hide_island(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("island") {
        let _ = win.hide();
    }
}

pub fn toggle_island(app: &AppHandle, state: &AppState) -> Result<bool, String> {
    let mut settings = state.db.get_settings()?;
    settings.island_enabled = !settings.island_enabled;
    let _ = state
        .db
        .set_settings(serde_json::json!({ "islandEnabled": settings.island_enabled }))?;
    if settings.island_enabled {
        show_island(app, state)?;
    } else {
        hide_island(app);
    }
    let _ = app.emit("settings:changed", settings.clone());
    Ok(settings.island_enabled)
}

pub fn start_clickthrough_loop(app: AppHandle) {
    std::thread::spawn(move || {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::{HWND, POINT, RECT};
            use windows::Win32::UI::WindowsAndMessaging::{
                GetCursorPos, GetWindowLongPtrW, GetWindowRect, IsWindowVisible, SetWindowLongPtrW,
                SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                WS_EX_TRANSPARENT,
            };

            let mut last_ignore: Option<bool> = None;
            let mut hwnd_bits: isize = 0;
            let mut last_pill = (0i32, 0i32);
            let mut last_topmost_ms: u64 = 0;
            let mut was_pass_through = false;
            loop {
                let pass_through = PASS_THROUGH.load(Ordering::Relaxed) != 0;
                let force_top = FORCE_TOPMOST.load(Ordering::Relaxed) != 0;

                // In-game HUD: almost idle. Re-pinning TOPMOST / toggling styles at 30 Hz
                // hitchs Vulkan/DWM titles (RDR2) without changing measured FPS.
                if pass_through {
                    if !was_pass_through {
                        if let Some(win) = app.get_webview_window("island") {
                            let _ = win.set_ignore_cursor_events(true);
                            force_pass_through_hwnd(&win);
                        }
                        was_pass_through = true;
                        last_ignore = Some(true);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    // Rare quiet topmost nudge (no SWP_SHOWWINDOW).
                    if force_top && now.saturating_sub(last_topmost_ms) >= 15_000 {
                        last_topmost_ms = now;
                        if hwnd_bits != 0 {
                            let hwnd = HWND(hwnd_bits as _);
                            unsafe {
                                let _ = SetWindowPos(
                                    hwnd,
                                    HWND_TOPMOST,
                                    0,
                                    0,
                                    0,
                                    0,
                                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                                );
                            }
                        }
                    }
                    continue;
                }
                was_pass_through = false;

                std::thread::sleep(std::time::Duration::from_millis(50));
                let hwnd = if hwnd_bits != 0 {
                    HWND(hwnd_bits as _)
                } else {
                    let Some(win) = app.get_webview_window("island") else {
                        last_ignore = None;
                        continue;
                    };
                    let Ok(native) = win.hwnd() else {
                        continue;
                    };
                    hwnd_bits = native.0 as isize;
                    HWND(native.0 as _)
                };
                unsafe {
                    if !IsWindowVisible(hwnd).as_bool() {
                        last_ignore = None;
                        hwnd_bits = 0;
                        continue;
                    }

                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    if force_top && now.saturating_sub(last_topmost_ms) >= 5_000 {
                        last_topmost_ms = now;
                        let _ = SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }

                    let mut pt = POINT::default();
                    if GetCursorPos(&mut pt).is_err() {
                        continue;
                    }
                    let mut rect = RECT::default();
                    if GetWindowRect(hwnd, &mut rect).is_err() {
                        continue;
                    }
                    let pill_w = PILL_W.load(Ordering::Relaxed) as i32;
                    let pill_h = PILL_H.load(Ordering::Relaxed) as i32;
                    if (pill_w, pill_h) != last_pill {
                        last_pill = (pill_w, pill_h);
                        last_ignore = None;
                    }
                    let host_w = rect.right - rect.left;
                    let px = rect.left + (host_w - pill_w) / 2;
                    let py = rect.top + 6;
                    let inside = pt.x >= px
                        && pt.y >= py
                        && pt.x <= px + pill_w
                        && pt.y <= py + pill_h;
                    let ignore = !inside;
                    if last_ignore == Some(ignore) {
                        continue;
                    }
                    last_ignore = Some(ignore);
                    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                    let bit = WS_EX_TRANSPARENT.0 as isize;
                    let next = if ignore { ex | bit } else { ex & !bit };
                    if next != ex {
                        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
                    }
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = app;
        }
    });
}

pub fn apply_main_effects(win: &WebviewWindow, blur: &str) {
    let _ = win.set_shadow(true);

    #[cfg(windows)]
    {
        use window_vibrancy::{
            apply_acrylic, apply_mica, apply_tabbed, clear_acrylic, clear_blur, clear_mica,
            clear_tabbed,
        };
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Dwm::{
            DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_BORDER_COLOR,
            DWMWA_CAPTION_COLOR, DWMWA_COLOR_NONE, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_TEXT_COLOR,
            DWMWA_USE_IMMERSIVE_DARK_MODE, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW,
        };
        use windows::Win32::UI::Controls::MARGINS;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_TOP,
            SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_LAYERED,
        };

        let _ = clear_blur(win);
        let _ = clear_acrylic(win);
        let _ = clear_mica(win);
        let _ = clear_tabbed(win);

        let glass = blur != "solid" && blur != "none";

        if let Ok(native) = win.hwnd() {
            let hwnd = HWND(native.0 as _);
            unsafe {
                // Classic WS_EX_LAYERED blocks Win11 DWM system backdrops.
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let layered = WS_EX_LAYERED.0 as isize;
                if ex & layered != 0 {
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !layered);
                    let _ = SetWindowPos(
                        hwnd,
                        HWND_TOP,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                    );
                }

                // Extend the glass into the whole client area (undecorated window).
                let margins = if glass {
                    MARGINS {
                        cxLeftWidth: -1,
                        cxRightWidth: -1,
                        cyTopHeight: -1,
                        cyBottomHeight: -1,
                    }
                } else {
                    MARGINS::default()
                };
                let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

                let dark: u32 = 1;
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_USE_IMMERSIVE_DARK_MODE,
                    &dark as *const _ as *const _,
                    4,
                );

                // Hide the Win11 caption/border tint (pink strip).
                let none = DWMWA_COLOR_NONE;
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_CAPTION_COLOR,
                    &none as *const _ as *const _,
                    4,
                );
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_BORDER_COLOR,
                    &none as *const _ as *const _,
                    4,
                );
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_TEXT_COLOR,
                    &none as *const _ as *const _,
                    4,
                );

                if !glass {
                    let disable = DWMSBT_NONE;
                    let _ = DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_SYSTEMBACKDROP_TYPE,
                        &disable as *const _ as *const _,
                        4,
                    );
                }
            }
        }

        if !glass {
            let _ = win.set_background_color(Some(Color(14, 14, 16, 255)));
            return;
        }

        // Alpha-0 WebView2 so the DWM backdrop (real desktop) shows through CSS.
        let _ = win.set_background_color(Some(Color(0, 0, 0, 0)));
        match blur {
            "mica" => {
                let _ = apply_mica(win, Some(true));
            }
            "tabbed" => {
                let _ = apply_tabbed(win, Some(true));
            }
            _ => {
                // Win11 acrylic = DWMSBT_TRANSIENTWINDOW — this is what Nebula uses.
                if let Ok(native) = win.hwnd() {
                    let hwnd = HWND(native.0 as _);
                    unsafe {
                        let acrylic = DWMSBT_TRANSIENTWINDOW;
                        let _ = DwmSetWindowAttribute(
                            hwnd,
                            DWMWA_SYSTEMBACKDROP_TYPE,
                            &acrylic as *const _ as *const _,
                            4,
                        );
                    }
                }
                let _ = apply_acrylic(win, Some((18, 18, 24, 100)));
            }
        }
    }

    #[cfg(not(windows))]
    {
        if blur == "solid" || blur == "none" {
            let _ = win.set_background_color(Some(Color(14, 14, 16, 255)));
        } else {
            let _ = win.set_background_color(Some(Color(0, 0, 0, 0)));
        }
    }
}
