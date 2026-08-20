use crate::games::AppState;
use std::sync::atomic::{AtomicU32, AtomicU16, Ordering};
use tauri::{
    window::Color,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub static UI_PORT: AtomicU16 = AtomicU16::new(47823);
static PILL_W: AtomicU32 = AtomicU32::new(196);
static PILL_H: AtomicU32 = AtomicU32::new(38);

/// Transparent host that the pill morphs inside. Never resize this — CSS animates the pill.
const HOST_W: u32 = 460;
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

    let x = pos.x + (size.width as i32 - HOST_W as i32) / 2;
    let y = pos.y + HOST_TOP;
    win.set_size(PhysicalSize::new(HOST_W, HOST_H))
        .map_err(|e| e.to_string())?;
    win.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    let _ = win.set_always_on_top(settings.island_on_top);
    Ok(())
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
                GWL_EXSTYLE, WS_EX_TRANSPARENT,
            };

            let mut last_ignore: Option<bool> = None;
            let mut hwnd_bits: isize = 0;
            let mut last_pill = (0i32, 0i32);
            loop {
                std::thread::sleep(std::time::Duration::from_millis(32));
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
