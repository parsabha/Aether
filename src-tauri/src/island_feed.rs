use crate::games::AppState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IslandNotification {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub game_id: Option<String>,
    pub app: Option<String>,
    pub created_at: i64,
    pub read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IslandMedia {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub app_name: String,
    pub status: String,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub thumbnail_data_url: Option<String>,
    /// "smtc" when Windows exposes track metadata, "audio" when we only know
    /// which app is producing sound (players that skip the media API).
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IslandFeed {
    pub media: Option<IslandMedia>,
    pub notifications: Vec<IslandNotification>,
    /// False when Windows has not granted notification-listener access.
    pub system_access: bool,
}

static NOTIF_SEQ: AtomicU64 = AtomicU64::new(1);

const SYSTEM_PREFIX: &str = "win-";
/// System toasts the user has already looked at, and ones they cleared away.
static SEEN_SYSTEM: parking_lot::Mutex<Vec<String>> = parking_lot::Mutex::new(Vec::new());
static HIDDEN_SYSTEM: parking_lot::Mutex<Vec<String>> = parking_lot::Mutex::new(Vec::new());
/// Whatever was already sitting in the Action Center at launch starts as read,
/// so the island only lights up for toasts that arrive while it is running.
static SYSTEM_BASELINED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(windows)]
static SYSTEM_CACHE: parking_lot::Mutex<Option<(i64, bool, Vec<IslandNotification>)>> =
    parking_lot::Mutex::new(None);

#[cfg(windows)]
static ART_CACHE: parking_lot::Mutex<Option<(String, Option<String>)>> =
    parking_lot::Mutex::new(None);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn new_id() -> String {
    let n = NOTIF_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("n-{n}-{}", now_ms())
}

pub fn notifications(state: &AppState) -> parking_lot::MutexGuard<'_, Vec<IslandNotification>> {
    state.notifications.lock()
}

pub fn push_notification(app: &AppHandle, state: &AppState, n: IslandNotification) {
    {
        let mut list = notifications(state);
        list.insert(0, n.clone());
        list.truncate(24);
    }
    let _ = app.emit("island:feed", ());
}

pub fn notify_screenshot(app: &AppHandle, state: &AppState, game_id: &str, game_name: &str) {
    push_notification(
        app,
        state,
        IslandNotification {
            id: new_id(),
            kind: "screenshot".into(),
            title: "Screenshot saved".into(),
            body: game_name.to_string(),
            game_id: Some(game_id.to_string()),
            app: None,
            created_at: now_ms(),
            read: false,
        },
    );
}

pub fn notify_session_end(
    app: &AppHandle,
    state: &AppState,
    game_id: &str,
    game_name: &str,
    duration_ms: i64,
) {
    if duration_ms < 60_000 {
        return;
    }
    let mins = duration_ms / 60_000;
    push_notification(
        app,
        state,
        IslandNotification {
            id: new_id(),
            kind: "session".into(),
            title: "Session ended".into(),
            body: format!("{game_name} · {mins} min"),
            game_id: Some(game_id.to_string()),
            app: None,
            created_at: now_ms(),
            read: false,
        },
    );
}

pub fn get_feed(state: &AppState) -> IslandFeed {
    let mut list = notifications(state).clone();

    #[cfg(windows)]
    let system_access = {
        let (allowed, system) = read_system_notifications();
        list.extend(system);
        allowed
    };
    #[cfg(not(windows))]
    let system_access = false;

    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    list.truncate(24);

    IslandFeed {
        media: read_media_session(),
        notifications: list,
        system_access,
    }
}

#[tauri::command]
pub fn island_feed(state: State<'_, AppState>) -> Result<IslandFeed, String> {
    Ok(get_feed(&state))
}

#[tauri::command]
pub fn island_dismiss_notification(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    if id.starts_with(SYSTEM_PREFIX) {
        {
            let mut seen = SEEN_SYSTEM.lock();
            if !seen.iter().any(|e| e == &id) {
                seen.push(id);
                seen.truncate(400);
            }
        }
        #[cfg(windows)]
        {
            *SYSTEM_CACHE.lock() = None;
        }
        return Ok(true);
    }
    let mut list = notifications(&state);
    if let Some(n) = list.iter_mut().find(|n| n.id == id) {
        n.read = true;
    }
    Ok(true)
}

#[tauri::command]
pub fn island_clear_notifications(state: State<'_, AppState>) -> Result<bool, String> {
    notifications(&state).clear();
    #[cfg(windows)]
    {
        let system = read_system_notifications().1;
        let mut hidden = HIDDEN_SYSTEM.lock();
        for n in system {
            if !hidden.iter().any(|e| e == &n.id) {
                hidden.push(n.id);
            }
        }
        hidden.truncate(400);
        *SYSTEM_CACHE.lock() = None;
    }
    Ok(true)
}

#[tauri::command]
pub fn island_mark_notifications_read(state: State<'_, AppState>) -> Result<bool, String> {
    for n in notifications(&state).iter_mut() {
        n.read = true;
    }
    #[cfg(windows)]
    {
        let system = read_system_notifications().1;
        let mut seen = SEEN_SYSTEM.lock();
        for n in system {
            if !seen.iter().any(|e| e == &n.id) {
                seen.push(n.id);
            }
        }
        seen.truncate(400);
        *SYSTEM_CACHE.lock() = None;
    }
    Ok(true)
}

#[tauri::command]
pub fn island_media_toggle() -> Result<bool, String> {
    media_toggle()
}

#[tauri::command]
pub fn island_media_next() -> Result<bool, String> {
    media_skip_next()
}

#[tauri::command]
pub fn island_media_previous() -> Result<bool, String> {
    media_skip_previous()
}

fn read_media_session() -> Option<IslandMedia> {
    #[cfg(windows)]
    {
        if let Some(m) = read_media_session_windows() {
            if m.status == "playing" {
                return Some(m);
            }
            return read_audio_session_windows().or(Some(m));
        }
        return read_audio_session_windows();
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn media_toggle() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let ok = with_current_session(|session| {
            let op = session
                .TryTogglePlayPauseAsync()
                .map_err(|e| e.to_string())?;
            op.get().map_err(|e| e.to_string())
        });
        return match ok {
            Ok(true) => Ok(true),
            _ => {
                send_media_key(windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_PLAY_PAUSE);
                Ok(true)
            }
        };
    }
    #[cfg(not(windows))]
    {
        Err("Media controls unavailable".into())
    }
}

fn media_skip_next() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let ok = with_current_session(|session| {
            let op = session.TrySkipNextAsync().map_err(|e| e.to_string())?;
            op.get().map_err(|e| e.to_string())
        });
        return match ok {
            Ok(true) => Ok(true),
            _ => {
                send_media_key(windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_NEXT_TRACK);
                Ok(true)
            }
        };
    }
    #[cfg(not(windows))]
    {
        Err("Media controls unavailable".into())
    }
}

fn media_skip_previous() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let ok = with_current_session(|session| {
            let op = session
                .TrySkipPreviousAsync()
                .map_err(|e| e.to_string())?;
            op.get().map_err(|e| e.to_string())
        });
        return match ok {
            Ok(true) => Ok(true),
            _ => {
                send_media_key(windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_PREV_TRACK);
                Ok(true)
            }
        };
    }
    #[cfg(not(windows))]
    {
        Err("Media controls unavailable".into())
    }
}

/// Players that skip the Windows media API usually still honour media keys.
#[cfg(windows)]
fn send_media_key(key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    };
    unsafe {
        keybd_event(key.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(key.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

#[cfg(windows)]
fn with_current_session<F>(f: F) -> Result<bool, String>
where
    F: FnOnce(
        windows::Media::Control::GlobalSystemMediaTransportControlsSession,
    ) -> Result<bool, String>,
{
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .map_err(|e| e.to_string())?
        .get()
        .map_err(|e| e.to_string())?;
    let session = manager
        .GetCurrentSession()
        .map_err(|e| e.to_string())?;
    f(session)
}

#[cfg(windows)]
fn read_media_session_windows() -> Option<IslandMedia> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .ok()?
        .get()
        .ok()?;

    // Prefer the OS "current" session, but fall back to any session that is
    // actually producing sound — browsers often are not the current session.
    let mut best: Option<IslandMedia> = manager
        .GetCurrentSession()
        .ok()
        .and_then(|s| media_from_session(&s));
    if best.as_ref().map(|m| m.status != "playing").unwrap_or(true) {
        if let Ok(sessions) = manager.GetSessions() {
            let count = sessions.Size().unwrap_or(0);
            for i in 0..count {
                let Ok(session) = sessions.GetAt(i) else {
                    continue;
                };
                if let Some(m) = media_from_session(&session) {
                    if m.status == "playing" {
                        return Some(m);
                    }
                    if best.is_none() {
                        best = Some(m);
                    }
                }
            }
        }
    }
    best
}

#[cfg(windows)]
fn media_from_session(
    session: &windows::Media::Control::GlobalSystemMediaTransportControlsSession,
) -> Option<IslandMedia> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status;
    use windows::Storage::Streams::DataReader;

    let props = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = props.Title().map(|t| t.to_string()).unwrap_or_default();
    if title.trim().is_empty() {
        return None;
    }

    let artist = props.Artist().map(|t| t.to_string()).unwrap_or_default();
    let album = props.AlbumTitle().map(|t| t.to_string()).unwrap_or_default();
    let app_name = session
        .SourceAppUserModelId()
        .map(|t| pretty_app_name(&t.to_string()))
        .unwrap_or_default();

    let status = session
        .GetPlaybackInfo()
        .and_then(|p| p.PlaybackStatus())
        .map(|s| match s {
            Status::Playing | Status::Changing => "playing",
            Status::Paused => "paused",
            _ => "stopped",
        })
        .unwrap_or("stopped")
        .to_string();

    let (position_ms, duration_ms) = session
        .GetTimelineProperties()
        .map(|t| {
            let pos = t.Position().map(|v| v.Duration / 10_000).unwrap_or(0);
            let end = t.EndTime().map(|v| v.Duration / 10_000).unwrap_or(0);
            (pos, end)
        })
        .unwrap_or((0, 0));

    // Re-encoding artwork on every poll is wasteful; keep the last track's.
    let art_key = format!("{title}|{artist}|{app_name}");
    let cached = ART_CACHE.lock().clone();
    let thumbnail_data_url = match cached {
        Some((key, art)) if key == art_key => art,
        _ => {
            let art = props.Thumbnail().ok().and_then(|thumb| {
                let stream = thumb.OpenReadAsync().ok()?.get().ok()?;
                let size = stream.Size().ok()? as u32;
                if size == 0 || size > 1_048_576 {
                    return None;
                }
                let reader = DataReader::CreateDataReader(&stream).ok()?;
                reader.LoadAsync(size).ok()?.get().ok()?;
                let mut bytes = vec![0u8; size as usize];
                reader.ReadBytes(&mut bytes).ok()?;
                let b64 =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
                Some(format!("data:image/png;base64,{b64}"))
            });
            *ART_CACHE.lock() = Some((art_key, art.clone()));
            art
        }
    };

    Some(IslandMedia {
        title,
        artist,
        album,
        app_name,
        status,
        position_ms,
        duration_ms,
        thumbnail_data_url,
        source: "smtc".into(),
    })
}

/// Real Windows toasts via the notification listener. Unpackaged apps can read
/// (but not subscribe to) this API, which is why the island polls instead.
#[cfg(windows)]
fn read_system_notifications() -> (bool, Vec<IslandNotification>) {
    const TTL_MS: i64 = 1_500;
    if let Some((at, allowed, cached)) = SYSTEM_CACHE.lock().clone() {
        if now_ms() - at < TTL_MS {
            return (allowed, cached);
        }
    }
    let fresh = read_system_notifications_uncached();
    *SYSTEM_CACHE.lock() = Some((now_ms(), fresh.0, fresh.1.clone()));
    fresh
}

#[cfg(windows)]
fn read_system_notifications_uncached() -> (bool, Vec<IslandNotification>) {
    use std::sync::atomic::Ordering as AtomicOrdering;
    use windows::UI::Notifications::Management::{
        UserNotificationListener, UserNotificationListenerAccessStatus,
    };
    use windows::UI::Notifications::{KnownNotificationBindings, NotificationKinds};

    let Ok(listener) = UserNotificationListener::Current() else {
        return (false, Vec::new());
    };

    let mut status = listener
        .GetAccessStatus()
        .unwrap_or(UserNotificationListenerAccessStatus::Unspecified);
    if status != UserNotificationListenerAccessStatus::Allowed {
        status = listener
            .RequestAccessAsync()
            .ok()
            .and_then(|op| op.get().ok())
            .unwrap_or(UserNotificationListenerAccessStatus::Denied);
    }
    if status != UserNotificationListenerAccessStatus::Allowed {
        return (false, Vec::new());
    }

    let Some(items) = listener
        .GetNotificationsAsync(NotificationKinds::Toast)
        .ok()
        .and_then(|op| op.get().ok())
    else {
        return (true, Vec::new());
    };

    let generic = KnownNotificationBindings::ToastGeneric().unwrap_or_default();
    let seen = SEEN_SYSTEM.lock().clone();
    let hidden = HIDDEN_SYSTEM.lock().clone();
    let count = items.Size().unwrap_or(0);
    let mut out = Vec::new();

    for i in 0..count {
        let Ok(item) = items.GetAt(i) else { continue };
        let id = format!("{SYSTEM_PREFIX}{}", item.Id().unwrap_or(0));
        if hidden.iter().any(|e| e == &id) {
            continue;
        }

        let app = item
            .AppInfo()
            .and_then(|a| a.DisplayInfo())
            .and_then(|d| d.DisplayName())
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "Windows".into());

        let mut lines: Vec<String> = Vec::new();
        if let Ok(binding) = item
            .Notification()
            .and_then(|n| n.Visual())
            .and_then(|v| v.GetBinding(&generic))
        {
            if let Ok(texts) = binding.GetTextElements() {
                for t in 0..texts.Size().unwrap_or(0) {
                    if let Ok(el) = texts.GetAt(t) {
                        let s = el.Text().map(|s| s.to_string()).unwrap_or_default();
                        if !s.trim().is_empty() {
                            lines.push(s);
                        }
                    }
                }
            }
        }
        if lines.is_empty() {
            continue;
        }

        let title = lines.remove(0);
        let body = if lines.is_empty() {
            app.clone()
        } else {
            lines.join(" · ")
        };

        // WinRT DateTime counts 100ns ticks from 1601-01-01.
        let created_at = item
            .CreationTime()
            .map(|d| d.UniversalTime / 10_000 - 11_644_473_600_000)
            .unwrap_or_else(|_| now_ms());

        out.push(IslandNotification {
            read: seen.iter().any(|e| e == &id),
            id,
            kind: "system".into(),
            title,
            body,
            game_id: None,
            app: Some(app),
            created_at,
        });
    }

    if !SYSTEM_BASELINED.swap(true, AtomicOrdering::Relaxed) {
        let mut seen = SEEN_SYSTEM.lock();
        for n in out.iter_mut() {
            n.read = true;
            seen.push(n.id.clone());
        }
        seen.truncate(400);
    }

    (true, out)
}

/// Fallback for players that never publish media metadata (KMPlayer, MPC, some
/// games): find the loudest audio session and describe it by window title.
#[cfg(windows)]
fn read_audio_session_windows() -> Option<IslandMedia> {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, AudioSessionStateActive, IAudioSessionControl2,
        IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .ok()?;
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None).ok()?;
        let sessions = manager.GetSessionEnumerator().ok()?;
        let count = sessions.GetCount().unwrap_or(0);
        let me = std::process::id();

        let mut best: Option<(f32, u32)> = None;
        for i in 0..count {
            let Ok(ctrl) = sessions.GetSession(i) else {
                continue;
            };
            if ctrl.GetState().ok()? != AudioSessionStateActive {
                continue;
            }
            let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else {
                continue;
            };
            let pid = ctrl2.GetProcessId().unwrap_or(0);
            if pid == 0 || pid == me {
                continue;
            }
            let peak = ctrl
                .cast::<IAudioMeterInformation>()
                .ok()
                .and_then(|m| m.GetPeakValue().ok())
                .unwrap_or(0.0);
            if peak <= 0.0008 {
                continue;
            }
            if best.map(|(p, _)| peak > p).unwrap_or(true) {
                best = Some((peak, pid));
            }
        }

        let (_, pid) = best?;
        let exe = process_exe_name(pid);
        let app = if exe.trim().is_empty() {
            "Audio".to_string()
        } else {
            pretty_app_name(&exe)
        };
        let title = window_title_for_pid(pid)
            .map(|t| strip_app_suffix(&t))
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| app.clone());

        Some(IslandMedia {
            title,
            artist: app.clone(),
            album: String::new(),
            app_name: app,
            status: "playing".into(),
            position_ms: 0,
            duration_ms: 0,
            thumbnail_data_url: None,
            source: "audio".into(),
        })
    }
}

#[cfg(windows)]
fn process_exe_name(pid: u32) -> String {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

#[cfg(windows)]
struct TitleHunt {
    pid: u32,
    title: Option<String>,
}

#[cfg(windows)]
unsafe extern "system" fn title_enum_proc(
    hwnd: windows::Win32::Foundation::HWND,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::BOOL {
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    let hunt = &mut *(lparam.0 as *mut TitleHunt);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid != hunt.pid || !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return BOOL(1);
    }
    let mut buf = vec![0u16; len as usize + 1];
    let read = GetWindowTextW(hwnd, &mut buf);
    if read > 0 {
        let title = String::from_utf16_lossy(&buf[..read as usize]);
        if !title.trim().is_empty() {
            hunt.title = Some(title);
            return BOOL(0);
        }
    }
    BOOL(1)
}

#[cfg(windows)]
fn window_title_for_pid(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;

    let mut hunt = TitleHunt { pid, title: None };
    unsafe {
        let _ = EnumWindows(
            Some(title_enum_proc),
            LPARAM(&mut hunt as *mut TitleHunt as isize),
        );
    }
    hunt.title
}

/// "Some Song - YouTube - Google Chrome" → "Some Song - YouTube"
#[cfg(windows)]
fn strip_app_suffix(title: &str) -> String {
    const SUFFIXES: [&str; 8] = [
        " - Google Chrome",
        " — Mozilla Firefox",
        " - Mozilla Firefox",
        " and 1 more page - Personal - Microsoft​ Edge",
        " - Microsoft Edge",
        " - Brave",
        " - Opera",
        " - Spotify",
    ];
    let mut out = title.trim().to_string();
    for s in SUFFIXES {
        if let Some(stripped) = out.strip_suffix(s) {
            out = stripped.trim().to_string();
        }
    }
    out
}

#[cfg(windows)]
fn pretty_app_name(aumid: &str) -> String {
    let raw = aumid
        .split('!')
        .next()
        .unwrap_or(aumid)
        .rsplit('\\')
        .next()
        .unwrap_or(aumid);
    let lower = raw.to_ascii_lowercase();
    for (needle, label) in [
        ("spotify", "Spotify"),
        ("chrome", "Chrome"),
        ("msedge", "Edge"),
        ("firefox", "Firefox"),
        ("brave", "Brave"),
        ("vlc", "VLC"),
        ("zunemusic", "Media Player"),
        ("mediaplayer", "Media Player"),
        ("apple", "Apple Music"),
        ("itunes", "iTunes"),
        ("mpc", "MPC"),
        ("potplayer", "PotPlayer"),
        ("foobar", "foobar2000"),
        ("aimp", "AIMP"),
        ("youtube", "YouTube"),
    ] {
        if lower.contains(needle) {
            return label.into();
        }
    }
    raw.trim_end_matches(".exe").to_string()
}
