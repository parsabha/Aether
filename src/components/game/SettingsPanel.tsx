import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import { primarySupportUrl } from "../../lib/links";
import type { DisplayInfo, Settings } from "../../lib/types";

const ACCENTS = [
  "#0078f2",
  "#7c5cff",
  "#3dd68c",
  "#ff6b4a",
  "#f5c542",
  "#22d3ee",
  "#ff4d8d",
  "#f97316",
];

const MATERIALS: { id: string; label: string; hint: string }[] = [
  { id: "solid", label: "Solid", hint: "Opaque dark" },
  { id: "acrylic", label: "Acrylic", hint: "Desktop through glass" },
  { id: "mica", label: "Mica", hint: "Desktop wallpaper tint" },
  { id: "tabbed", label: "Tabbed", hint: "Win 11 sheet" },
];

const THEMES: { id: string; label: string; hint: string }[] = [
  { id: "aether", label: "Aether", hint: "Glass aurora" },
  { id: "skeuo", label: "Skeuomorphism", hint: "Precision instrument" },
  { id: "flat", label: "Flat", hint: "Swiss color and line" },
  { id: "neu", label: "Neumorphism", hint: "Soft extruded surface" },
  { id: "material", label: "Material", hint: "Tonal elevation" },
  { id: "clay", label: "Clay", hint: "Inflated soft 3D" },
  { id: "prism", label: "Prism", hint: "Iridescent glass" },
  { id: "noir", label: "Noir", hint: "Cinematic OLED" },
  { id: "paper", label: "Paper", hint: "Editorial light" },
];

function Toggle({
  on,
  onChange,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      type="button"
      className={`toggle ${on ? "on" : ""}`}
      role="switch"
      aria-checked={on}
      onClick={() => onChange(!on)}
    >
      <i />
    </button>
  );
}

export function SettingsPanel({
  settings,
  onChange,
  onClose,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
  onClose: () => void;
}) {
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [ffmpeg, setFfmpeg] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    api.getDisplays().then(setDisplays).catch(() => {});
    api.ffmpegAvailable().then(setFfmpeg).catch(() => {});
  }, []);

  const patch = async (p: Partial<Settings>) => {
    try {
      const s = await api.setSettings(p);
      onChange(s);
      setMsg(null);
      return s;
    } catch (e) {
      setMsg(e instanceof Error ? e.message : String(e));
      return null;
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
        <div className="settings-head">
          <div>
            <h2>Settings</h2>
            <p className="settings-sub">Appearance, island, and how Aether launches</p>
          </div>
          <button className="tb-btn" onClick={onClose} aria-label="Close">✕</button>
        </div>

        <section className="set-section">
          <h3>Appearance</h3>
          <div className="set-copy" style={{ margin: "4px 0 8px" }}>
            <strong>Theme</strong>
            <span>Visual language for the whole launcher</span>
          </div>
          <div className="theme-grid">
            {THEMES.map((t) => (
              <button
                key={t.id}
                type="button"
                className={`theme-card is-${t.id} ${(settings.theme || "aether") === t.id ? "active" : ""}`}
                onClick={() => patch({ theme: t.id })}
              >
                <span className="theme-preview" aria-hidden>
                  <span className="tp-chrome">
                    <i /><i /><i />
                  </span>
                  <span className="tp-body">
                    <i className="tp-rail" />
                    <span className="tp-tiles">
                      <i className="tp-tile" />
                      <i className="tp-tile" />
                    </span>
                  </span>
                </span>
                <b>{t.label}</b>
                <small>{t.hint}</small>
              </button>
            ))}
          </div>
          <div className="set-row">
            <div className="set-copy">
              <strong>Accent</strong>
              <span>Used across the launcher</span>
            </div>
            <input
              type="color"
              value={settings.accent}
              onChange={(e) => patch({ accent: e.target.value })}
            />
          </div>
          <div className="swatches set-swatches">
            {ACCENTS.map((c) => (
              <button
                key={c}
                type="button"
                className={`swatch ${settings.accent.toLowerCase() === c.toLowerCase() ? "active" : ""}`}
                style={{ background: c }}
                onClick={() => patch({ accent: c })}
              />
            ))}
          </div>
          <div className="set-copy" style={{ margin: "12px 0 8px" }}>
            <strong>Window material</strong>
            <span>How the shell blends with your desktop</span>
          </div>
          <div className="material-grid">
            {MATERIALS.map((m) => (
              <button
                key={m.id}
                type="button"
                className={`material-card is-${m.id} ${settings.blur === m.id ? "active" : ""}`}
                onClick={() => patch({ blur: m.id })}
              >
                <span className="material-preview" />
                <b>{m.label}</b>
                <small>{m.hint}</small>
              </button>
            ))}
          </div>
          <div className="set-row">
            <div className="set-copy">
              <strong>Reduce motion</strong>
              <span>Less animation in the UI</span>
            </div>
            <Toggle on={settings.reduceMotion} onChange={(v) => patch({ reduceMotion: v })} />
          </div>
        </section>

        <section className="set-section">
          <h3>Dynamic Island</h3>
          <div className="set-row">
            <div className="set-copy">
              <strong>Show island</strong>
              <span>Compact launcher at the top of the screen</span>
            </div>
            <Toggle on={settings.islandEnabled} onChange={(v) => patch({ islandEnabled: v })} />
          </div>
          <div className="set-row">
            <div className="set-copy">
              <strong>Always on top</strong>
              <span>Keep the island above other windows</span>
            </div>
            <Toggle
              on={settings.islandOnTop}
              onChange={(v) => patch({ islandOnTop: v })}
            />
          </div>
          <div className="set-row">
            <div className="set-copy">
              <strong>Display</strong>
              <span>Which monitor hosts the island</span>
            </div>
            <select
              value={settings.islandDisplay ?? ""}
              onChange={(e) =>
                patch({
                  islandDisplay: e.target.value === "" ? null : Number(e.target.value),
                })
              }
            >
              <option value="">Primary</option>
              {displays.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name} ({d.width}×{d.height})
                </option>
              ))}
            </select>
          </div>
          <div className="set-row">
            <div className="set-copy">
              <strong>In-game overlay</strong>
              <span>
                While a library game is running (launched from Aether or elsewhere), pin the island
                above the game as a click-through stats HUD. Works with normal fullscreen.
              </span>
            </div>
            <Toggle
              on={!!settings.overlayEnabled}
              onChange={(v) => patch({ overlayEnabled: v })}
            />
          </div>
          {settings.overlayEnabled && (
            <>
              <div className="set-row set-row-nested">
                <div className="set-copy">
                  <strong>FPS</strong>
                  <span>Frames per second for the running game</span>
                </div>
                <Toggle
                  on={settings.overlayShowFps !== false}
                  onChange={(v) => patch({ overlayShowFps: v })}
                />
              </div>
              <div className="set-row set-row-nested">
                <div className="set-copy">
                  <strong>CPU usage</strong>
                  <span>System-wide processor load</span>
                </div>
                <Toggle
                  on={settings.overlayShowCpuUsage !== false}
                  onChange={(v) => patch({ overlayShowCpuUsage: v })}
                />
              </div>
              <div className="set-row set-row-nested">
                <div className="set-copy">
                  <strong>GPU usage</strong>
                  <span>Graphics engine utilization</span>
                </div>
                <Toggle
                  on={settings.overlayShowGpuUsage !== false}
                  onChange={(v) => patch({ overlayShowGpuUsage: v })}
                />
              </div>
              <div className="set-row set-row-nested">
                <div className="set-copy">
                  <strong>CPU temperature</strong>
                  <span>Package / thermal zone when Windows exposes it</span>
                </div>
                <Toggle
                  on={settings.overlayShowCpuTemp !== false}
                  onChange={(v) => patch({ overlayShowCpuTemp: v })}
                />
              </div>
              <div className="set-row set-row-nested">
                <div className="set-copy">
                  <strong>VRAM usage</strong>
                  <span>Dedicated video memory used / total</span>
                </div>
                <Toggle
                  on={settings.overlayShowVram !== false}
                  onChange={(v) => patch({ overlayShowVram: v })}
                />
              </div>
            </>
          )}
        </section>

        <section className="set-section">
          <h3>Launch</h3>
          <div className="set-row">
            <div className="set-copy">
              <strong>Open Aether on startup</strong>
              <span>Start with Windows</span>
            </div>
            <Toggle on={settings.launchOnStartup} onChange={(v) => patch({ launchOnStartup: v })} />
          </div>
          <div className="set-row">
            <div className="set-copy">
              <strong>Start in tray</strong>
              <span>Hide the main window until you open it</span>
            </div>
            <Toggle on={settings.startMinimized} onChange={(v) => patch({ startMinimized: v })} />
          </div>
          <div className="set-row">
            <div className="set-copy">
              <strong>Launch games as admin</strong>
              <span>Games inherit Aether’s administrator rights — Play opens them with no extra prompt</span>
            </div>
            <Toggle on={settings.launchAsAdmin} onChange={(v) => patch({ launchAsAdmin: v })} />
          </div>
          <div className="set-row">
            <div className="set-copy">
              <strong>Screenshot hotkey</strong>
              <span>Capture into the current game page</span>
            </div>
            <Toggle
              on={settings.screenshotHotkeyEnabled}
              onChange={(v) => patch({ screenshotHotkeyEnabled: v })}
            />
          </div>
          <div className="set-row">
            <div className="set-copy">
              <strong>Hotkey</strong>
            </div>
            <select
              value={settings.screenshotHotkey}
              onChange={(e) => patch({ screenshotHotkey: e.target.value })}
              disabled={!settings.screenshotHotkeyEnabled}
            >
              <option value="F9">F9</option>
              <option value="F8">F8</option>
              <option value="F10">F10</option>
              <option value="F11">F11</option>
              <option value="F12">F12</option>
              <option value="CTRL+SHIFT+S">Ctrl+Shift+S</option>
            </select>
          </div>
        </section>

        {primarySupportUrl() && (
          <section className="set-section">
            <h3>Support</h3>
            <div className="set-row">
              <div className="set-copy">
                <strong>Aether is independent</strong>
                <span>Donations keep the launcher going</span>
              </div>
              <button
                type="button"
                className="btn primary"
                onClick={() => {
                  const url = primarySupportUrl();
                  if (url) openUrl(url).catch((e) => setMsg(String(e)));
                }}
              >
                Donate
              </button>
            </div>
          </section>
        )}

        <section className="set-section">
          <h3>Files</h3>
          <div className="set-status">
            <span className={`status-dot ${ffmpeg ? "ok" : ""}`} />
            {ffmpeg ? "ffmpeg ready — GIF covers convert to WebM" : "ffmpeg not found — animated covers stay as GIFs"}
          </div>
          <div className="settings-actions">
            <button className="btn" onClick={() => api.openDataDir().catch((e) => setMsg(String(e)))}>
              Open data folder
            </button>
            <button className="btn primary" onClick={onClose}>
              Done
            </button>
          </div>
        </section>

        {msg && <p className="settings-msg">{msg}</p>}
      </div>
    </div>
  );
}
