use crate::db::Db;
use image::imageops::FilterType;
use image::ImageReader;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub struct Profile {
    pub dim: u32,
    pub duration_sec: u32,
}

pub fn profile_for(slot: &str) -> Profile {
    match slot {
        "cover" => Profile {
            dim: 480,
            duration_sec: 6,
        },
        "banner" => Profile {
            dim: 1280,
            duration_sec: 8,
        },
        "icon" => Profile {
            dim: 400,
            duration_sec: 4,
        },
        "screenshots" => Profile {
            dim: 1600,
            duration_sec: 8,
        },
        _ => Profile {
            dim: 800,
            duration_sec: 6,
        },
    }
}

fn ext_of(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn is_motion(path: &Path) -> bool {
    let ext = ext_of(path);
    matches!(ext.as_str(), "gif" | "webm" | "mp4" | "mov" | "mkv" | "avi")
        || looks_like_gif(path)
        || looks_like_animated_webp(path)
}

fn looks_like_gif(path: &Path) -> bool {
    if let Ok(bytes) = std::fs::read(path) {
        return bytes.len() >= 3 && &bytes[0..3] == b"GIF";
    }
    false
}

fn looks_like_animated_webp(path: &Path) -> bool {
    if let Ok(bytes) = std::fs::read(path) {
        let s = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]);
        return s.contains("WEBP") && s.contains("ANIM");
    }
    false
}

static FFMPEG_CACHE: OnceLock<Mutex<Option<Option<PathBuf>>>> = OnceLock::new();

fn ffmpeg_works(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let mut cmd = Command::new(path);
    cmd.arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    matches!(cmd.status(), Ok(s) if s.success())
}

fn candidate_ffmpeg_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();

    #[cfg(windows)]
    {
        if let Ok(output) = Command::new("where").arg("ffmpeg").output() {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    let p = PathBuf::from(line.trim());
                    if p.exists() {
                        out.push(p);
                    }
                }
            }
        }
        // Prefer full WinGet install over a lone copied exe (copied exe often misses DLLs → 0xc0000142)
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let winget = PathBuf::from(local).join("Microsoft\\WinGet\\Packages");
            if winget.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&winget) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.to_ascii_lowercase().contains("ffmpeg") {
                            let bin = entry.path().join("ffmpeg-9.0-full_build\\bin\\ffmpeg.exe");
                            if bin.exists() {
                                out.push(bin);
                            }
                            // also search one level for any */bin/ffmpeg.exe
                            if let Ok(walk) = std::fs::read_dir(entry.path()) {
                                for child in walk.flatten() {
                                    let cand = child.path().join("bin").join("ffmpeg.exe");
                                    if cand.exists() {
                                        out.push(cand);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(output) = Command::new("which").arg("ffmpeg").output() {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                let p = PathBuf::from(text.trim());
                if p.exists() {
                    out.push(p);
                }
            }
        }
    }

    if let Ok(appdata) = std::env::var("APPDATA") {
        out.push(PathBuf::from(appdata).join("Aether").join("bin").join("ffmpeg.exe"));
    }
    if let Some(data) = dirs::data_dir() {
        out.push(data.join("Aether").join("bin").join("ffmpeg.exe"));
        out.push(data.join("Aether").join("bin").join("ffmpeg"));
    }

    out
}

pub fn find_ffmpeg() -> Option<PathBuf> {
    let cache = FFMPEG_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock();
    if let Some(cached) = guard.as_ref() {
        return cached.clone();
    }
    let mut found = None;
    for cand in candidate_ffmpeg_paths() {
        if ffmpeg_works(&cand) {
            found = Some(cand);
            break;
        }
    }
    // Remove broken lone copies that crash with 0xc0000142
    if let Ok(appdata) = std::env::var("APPDATA") {
        let broken = PathBuf::from(appdata).join("Aether").join("bin").join("ffmpeg.exe");
        if broken.exists() && !ffmpeg_works(&broken) {
            let _ = std::fs::remove_file(&broken);
        }
    }
    *guard = Some(found.clone());
    found
}

pub fn optimize_slot(
    db: &Db,
    game_id: &str,
    slot: &str,
    src: &Path,
) -> Result<(Option<String>, Option<String>, Option<String>), String> {
    if !src.exists() {
        return Err(format!("missing media: {}", src.display()));
    }
    let dest = db.game_media_dir(game_id);
    let profile = profile_for(slot);
    let stamp = chrono::Utc::now().timestamp_millis();

    if is_motion(src) {
        let webm = dest.join(format!("{slot}-{stamp}-opt.webm"));
        let poster = dest.join(format!("{slot}-{stamp}-poster.jpg"));
        let mut webm_out: Option<String> = None;
        if let Some(ff) = find_ffmpeg() {
            let ok = run_ffmpeg(&ff, src, &webm, profile.dim, profile.duration_sec);
            if ok && webm.exists() && webm.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
                webm_out = Some(webm.to_string_lossy().to_string());
            } else {
                let _ = std::fs::remove_file(&webm);
            }
        }
        if !make_poster_ffmpeg(src, &poster, profile.dim) {
            let _ = downscale_static(src, &poster, profile.dim);
        }
        let poster_out = if poster.exists() {
            Some(poster.to_string_lossy().to_string())
        } else {
            None
        };
        let orig_ext = ext_of(src);
        let orig = dest.join(format!("{slot}-{stamp}-src.{orig_ext}"));
        let _ = std::fs::copy(src, &orig);
        Ok((
            Some(orig.to_string_lossy().to_string()),
            webm_out,
            poster_out,
        ))
    } else {
        let out = dest.join(format!("{slot}-{stamp}.jpg"));
        downscale_static(src, &out, profile.dim)?;
        Ok((
            Some(out.to_string_lossy().to_string()),
            None,
            Some(out.to_string_lossy().to_string()),
        ))
    }
}

fn run_ffmpeg(ff: &Path, src: &Path, out: &Path, dim: u32, duration: u32) -> bool {
    let vf = format!("fps=24,scale='min({dim},iw)':-2:flags=lanczos");
    let mut cmd = Command::new(ff);
    cmd.args([
        "-y",
        "-i",
        &src.to_string_lossy(),
        "-t",
        &duration.to_string(),
        "-vf",
        &vf,
        "-an",
        "-c:v",
        "libvpx-vp9",
        "-b:v",
        "0",
        "-crf",
        "34",
        "-deadline",
        "good",
        "-cpu-used",
        "4",
        "-pix_fmt",
        "yuv420p",
        &out.to_string_lossy(),
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    matches!(cmd.status(), Ok(s) if s.success())
}

fn make_poster_ffmpeg(src: &Path, out: &Path, dim: u32) -> bool {
    let Some(ff) = find_ffmpeg() else {
        return false;
    };
    let vf = format!("scale='min({dim},iw)':-2:flags=lanczos");
    let mut cmd = Command::new(ff);
    cmd.args([
        "-y",
        "-i",
        &src.to_string_lossy(),
        "-frames:v",
        "1",
        "-vf",
        &vf,
        &out.to_string_lossy(),
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    matches!(cmd.status(), Ok(s) if s.success()) && out.exists()
}

fn downscale_static(src: &Path, out: &Path, dim: u32) -> Result<(), String> {
    let img = ImageReader::open(src)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let resized = img.resize(dim, dim, FilterType::Lanczos3);
    let rgb = resized.to_rgb8();
    rgb.save(out).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn apply_optimized(
    db: &Db,
    game_id: &str,
    slot: &str,
    original: Option<String>,
    webm: Option<String>,
    poster: Option<String>,
) -> Result<(), String> {
    let mut g = db
        .get_game(game_id)?
        .ok_or_else(|| "game missing".to_string())?;
    match slot {
        "cover" => {
            if let Some(o) = original {
                g.cover = Some(o);
            }
            g.cover_webm = webm;
            g.cover_poster = poster;
        }
        "banner" => {
            if let Some(o) = original {
                g.banner = Some(o);
            }
            g.banner_webm = webm;
            g.banner_poster = poster;
        }
        "icon" => {
            if let Some(o) = original.or(poster) {
                g.icon = Some(o);
            }
        }
        _ => {}
    }
    db.upsert_game(&g)
}

pub fn process_pending(db: &Db) -> Result<usize, String> {
    // Skip motion jobs if ffmpeg is unavailable — avoids Windows 0xc0000142 popups.
    let has_ff = find_ffmpeg().is_some();
    let jobs = db.take_pending_jobs(4)?;
    let mut n = 0;
    for (id, game_id, slot, src) in jobs {
        let path = PathBuf::from(&src);
        if is_motion(&path) && !has_ff {
            // Re-queue later by marking error? Keep as pending by finishing false and not looping forever:
            // mark done with poster-only via image crate if possible.
            let poster = db.game_media_dir(&game_id).join(format!("{slot}-poster-fallback.jpg"));
            let _ = downscale_static(&path, &poster, profile_for(&slot).dim);
            let poster_s = if poster.exists() {
                Some(poster.to_string_lossy().to_string())
            } else {
                None
            };
            let _ = apply_optimized(
                db,
                &game_id,
                &slot,
                Some(src),
                None,
                poster_s,
            );
            let _ = db.finish_job(id, true);
            n += 1;
            continue;
        }
        match optimize_slot(db, &game_id, &slot, &path) {
            Ok((orig, webm, poster)) => {
                let _ = apply_optimized(db, &game_id, &slot, orig, webm, poster);
                let _ = db.finish_job(id, true);
                n += 1;
            }
            Err(_) => {
                let _ = db.finish_job(id, false);
            }
        }
    }
    Ok(n)
}

pub fn reoptimize_library(db: &Db) -> Result<usize, String> {
    let games = db.list_games()?;
    let mut n = 0;
    for g in games {
        for (slot, src) in [
            ("cover", g.cover.clone()),
            ("banner", g.banner.clone()),
            ("icon", g.icon.clone()),
        ] {
            if let Some(s) = src {
                let p = PathBuf::from(&s);
                if p.exists() {
                    match optimize_slot(db, &g.id, slot, &p) {
                        Ok((orig, webm, poster)) => {
                            let _ = apply_optimized(db, &g.id, slot, orig, webm, poster);
                            n += 1;
                        }
                        Err(_) => {}
                    }
                }
            }
        }
    }
    Ok(n)
}
