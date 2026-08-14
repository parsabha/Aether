# Aether

Liquid-glass game launcher for Windows, with a Dynamic Island on your desktop.

**Others can download and install Aether. They cannot change this repository** — only [parsabahemmat](https://github.com/parsabahemmat) can push updates.

## Install (Windows)

Download the latest installer from [Releases](https://github.com/parsabahemmat/Aether/releases):

**[Aether-Setup-1.0.0.exe](https://github.com/parsabahemmat/Aether/releases/latest)**

Run it (no administrator password). After install, Aether is in the Start menu. Hover the island at the top of the screen to open the hub.

## Support Aether

If Aether is useful, you can [sponsor the project on GitHub](https://github.com/sponsors/parsabahemmat).

## Data

Library files live in `%APPDATA%\Aether\`. First launch can import from `%APPDATA%\Nebula\` if that folder exists.

ffmpeg on PATH (or `ffmpeg.exe` in `%APPDATA%\Aether\bin\`) makes GIF/video covers play as WebM.

## Run from source

```bash
npm install
npm run tauri dev
```

Requires Rust (MSVC), Node 18+, and WebView2.

Build the branded installer:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-installer.ps1
```
