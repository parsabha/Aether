<p align="center">
  <img src="docs/logo-v5.png" width="160" height="160" alt="Aether">
</p>

<h1 align="center">Aether</h1>

<p align="center">
  Game launcher for Windows.<br>
  Full library in one window. A Dynamic Island on the desktop.
</p>

<p align="center">
  <a href="https://github.com/parsabahemmat/Aether/releases/latest"><strong>Download for Windows</strong></a>
  &nbsp;·&nbsp;
  <a href="https://github.com/parsabahemmat/Aether/releases/tag/v1.1.0">v1.1.0</a>
</p>

<p align="center">
  <img src="docs/screenshot.jpg" alt="Aether library" width="920">
</p>

---

Aether keeps your PC games in a local library with animated covers, playtime, and a redesigned detail page for each title. A compact island stays at the top of the screen so you can launch a game, skip a track, or check a notification without opening the main window.

Windows 10 and 11. Installs for the current user. The app runs elevated so Play can open games as administrator without a second UAC prompt when that option is on.

## What’s new in 1.1

- **Game page redesign** — brighter banner hero, artwork gallery (cover / banner / icon), screenshot grid, and game settings tucked behind Edit instead of crowding the page
- **Design themes** — Aether, Skeuomorphism, Flat, Neumorphism, Material, Clay, Prism, Noir, and Paper in Settings
- **Smoother launches** — library and media work moved off the UI thread; island feed and media session updates are more reliable
- **Admin play** — optional “launch games as admin” inherits Aether’s elevation so titles that need it start cleanly
- **Startup** — when elevation is required, Aether prefers a scheduled task so it can come back as admin after sign-in

## Library

Add games by dropping `.exe`, `.lnk`, `.bat`, or `.cmd` files onto the window, or pick them from disk. Each entry can have a cover, banner, icon, tags, category, and notes.

Covers accept still images, GIFs, and video. With [ffmpeg](https://ffmpeg.org/) on `PATH` (or `ffmpeg.exe` in `%APPDATA%\Aether\bin\`), GIF and video artwork is converted to VP9 WebM and plays while the tile is visible.

Playtime and launch count are stored on your machine. Search, favorites, and a custom sort order are included. If a Nebula library exists at `%APPDATA%\Nebula`, Aether imports it on first launch.

## Game page

Open a title for a cinematic banner, cover, play controls, about text, artwork section, and screenshots. Press the gear or **Edit game** to change name, media, accent, launch args, and the executable. A configurable hotkey (default F9) captures into that game’s screenshot gallery.

## Island

A pill sits at the top center of the display. Hover to expand it: running and recent games, now-playing controls from Windows media sessions, and a short notification list. Clicks outside the island pass through to the desktop.

The island can stay above other windows and can be assigned to a specific monitor.

## Appearance

Pick a visual theme and window material (Acrylic, Mica, Tabbed, or solid). Accent color, reduced motion, launch on sign-in, start minimized, and “run games as administrator” live in Settings.

## Install

1. Download [**Aether-Setup-1.1.0.exe**](https://github.com/parsabahemmat/Aether/releases/latest)
2. Run the installer (Windows may ask for administrator once)
3. Open **Aether** from the Start menu

The installer will set up [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) if it is missing. Library files live in `%APPDATA%\Aether`.

## Build from source

Rust (MSVC toolchain), Node 18+, and WebView2.

```bash
npm install
npm run tauri dev
```

Windows installer:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-installer.ps1
```

## License

See [LICENSE.txt](LICENSE.txt). [Sponsor](https://github.com/sponsors/parsabahemmat)
