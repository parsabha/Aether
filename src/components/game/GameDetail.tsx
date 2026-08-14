import { open } from "@tauri-apps/plugin-dialog";
import { motion } from "motion/react";
import { useEffect, useState, type CSSProperties } from "react";
import { api } from "../../lib/api";
import type { Game, MediaKind } from "../../lib/types";
import { AutoPlayMedia } from "../media/AutoPlayMedia";

const ACCENT_SWATCHES = [
  "#0078f2",
  "#3dd68c",
  "#ff6b4a",
  "#f5c542",
  "#a78bfa",
  "#ff4d8d",
  "#22d3ee",
  "#f97316",
  "#e2e8f0",
];

function fmtPlaytime(ms: number) {
  const h = Math.floor(ms / 3600000);
  const m = Math.floor((ms % 3600000) / 60000);
  if (h <= 0) return `${m}m`;
  return `${h}h ${m}m`;
}

function fmtDate(ms?: number | null) {
  if (!ms) return "—";
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return "—";
  }
}

function MediaTile({
  label,
  url,
  kind,
  tall,
  onPick,
}: {
  label: string;
  url?: string | null;
  kind?: MediaKind | string | null;
  tall?: boolean;
  onPick: () => void;
}) {
  return (
    <button type="button" className={`media-tile ${tall ? "tall" : ""}`} onClick={onPick}>
      {url ? (
        <AutoPlayMedia src={url} kind={kind || "image"} className="media-tile-media" />
      ) : (
        <div className="media-tile-empty">+</div>
      )}
      <span className="mt-label">
        <span>{label}</span>
        <span>{url ? "Change" : "Add"}</span>
      </span>
    </button>
  );
}

export function GameDetail({
  game,
  onBack,
  onChange,
  onLaunch,
  reduceMotion,
}: {
  game: Game;
  onBack: () => void;
  onChange: (g: Game) => void;
  onLaunch: () => void;
  reduceMotion?: boolean;
}) {
  const [lightbox, setLightbox] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [draft, setDraft] = useState({
    name: game.name,
    description: game.description,
    category: game.category,
    args: game.args,
    tags: game.tags.join(", "),
    accent: game.accent || "",
  });

  useEffect(() => {
    setDraft({
      name: game.name,
      description: game.description,
      category: game.category,
      args: game.args,
      tags: game.tags.join(", "),
      accent: game.accent || "",
    });
  }, [game.id, game.name, game.description, game.category, game.args, game.tags, game.accent]);

  const pageAccent = draft.accent || game.accent || undefined;

  const pick = async (slot: "cover" | "banner" | "icon") => {
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: "Media",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "webm", "mp4", "mov"],
        },
      ],
    });
    if (!selected || Array.isArray(selected)) return;
    const g = await api.setMediaPath(game.id, slot, selected);
    onChange(g);
  };

  const pickExe = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Games", extensions: ["exe", "lnk", "bat", "cmd"] }],
    });
    if (!selected || Array.isArray(selected)) return;
    const cwd = selected.replace(/[\\/][^\\/]+$/, "");
    const g = await api.updateGame(game.id, { exePath: selected, cwd });
    onChange(g);
  };

  const save = async () => {
    setSaving(true);
    try {
      const g = await api.updateGame(game.id, {
        name: draft.name.trim() || game.name,
        description: draft.description,
        category: draft.category.trim() || "Games",
        args: draft.args,
        tags: draft.tags
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
        accent: draft.accent || null,
      });
      onChange(g);
    } finally {
      setSaving(false);
    }
  };

  const banner = game.bannerUrl || game.coverUrl;
  const bannerKind = game.bannerUrl ? game.bannerKind : game.coverKind;

  return (
    <div
      className="detail"
      style={pageAccent ? ({ ["--accent" as string]: pageAccent } as CSSProperties) : undefined}
    >
      <div className="detail-hero">
        {banner ? (
          <AutoPlayMedia src={banner} kind={bannerKind} className="detail-hero-media" />
        ) : (
          <div
            className="detail-hero-media"
            style={{
              background: `linear-gradient(135deg, color-mix(in srgb, var(--accent) 40%, #101018), #08080a)`,
            }}
          />
        )}
        <div className="detail-hero-scrim" />
        <div className="detail-hero-content">
          <button className="back-btn" onClick={onBack}>
            ← Library
          </button>
          <div className="detail-hero-row">
            <div className="detail-cover">
              {game.coverUrl ? (
                <AutoPlayMedia
                  src={game.coverUrl}
                  kind={game.coverKind}
                  className="detail-cover-media"
                />
              ) : game.iconUrl ? (
                <img src={game.iconUrl} alt="" className="detail-cover-media" style={{ objectFit: "contain", padding: "18%" }} />
              ) : (
                <div className="game-card-ph" style={{ height: "100%" }}>
                  {game.name.slice(0, 1).toUpperCase()}
                </div>
              )}
            </div>
            <div className="detail-text">
              <h1>{draft.name || game.name}</h1>
              <div className="detail-meta-row">
                <span className="detail-chip">{game.category || "Games"}</span>
                {game.running && <span className="detail-chip live">Playing now</span>}
                {game.missing && <span className="detail-chip warn">Missing exe</span>}
                <span className="detail-meta-stat">
                  <b>{fmtPlaytime(game.playtimeMs)}</b> played
                </span>
                <span className="detail-meta-stat">
                  <b>{game.launchCount}</b> launches
                </span>
              </div>
              {(game.tags.length > 0 || draft.tags) && (
                <div className="tag-row">
                  {(draft.tags
                    ? draft.tags.split(",").map((t) => t.trim()).filter(Boolean)
                    : game.tags
                  ).map((t) => (
                    <span key={t} className="tag">
                      {t}
                    </span>
                  ))}
                </div>
              )}
              <div className="detail-actions">
                <motion.button
                  className="btn-play"
                  disabled={game.missing}
                  onClick={onLaunch}
                  whileTap={reduceMotion ? undefined : { scale: 0.97 }}
                >
                  <svg viewBox="0 0 24 24" aria-hidden>
                    <path d="M8 5.5v13l11-6.5L8 5.5z" />
                  </svg>
                  {game.missing ? "Missing" : game.running ? "Running" : "Play"}
                </motion.button>
                {game.missing && (
                  <button type="button" className="btn" onClick={pickExe}>
                    Locate exe
                  </button>
                )}
                <button
                  type="button"
                  className={`btn ${game.favorite ? "accent" : ""}`}
                  onClick={async () =>
                    onChange(await api.updateGame(game.id, { favorite: !game.favorite }))
                  }
                >
                  {game.favorite ? "★ Favorited" : "☆ Favorite"}
                </button>
                {game.exePath && (
                  <button type="button" className="btn" onClick={() => api.openFolder(game.exePath!)}>
                    Show in folder
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="detail-body">
        <div className="detail-main">
          <div className="panel">
            <h3>About</h3>
            {draft.description || game.description ? (
              <p className="detail-desc">{draft.description || game.description}</p>
            ) : (
              <p className="detail-desc" style={{ opacity: 0.55 }}>
                No description yet — add one in Customize.
              </p>
            )}
          </div>

          <div className="panel">
            <h3>Screenshots</h3>
            <div className="shots-row">
              {game.screenshotUrls.map((s) => (
                <div key={s.path} className="shot-item">
                  <img src={s.url} alt="" onClick={() => setLightbox(s.url)} />
                  <button
                    className="shot-del"
                    title="Delete"
                    onClick={async () => {
                      await api.removeScreenshot(game.id, s.path);
                      const lib = await api.getLibrary();
                      const fresh = lib.find((x) => x.id === game.id);
                      if (fresh) onChange(fresh);
                    }}
                  >
                    ✕
                  </button>
                </div>
              ))}
              {!game.screenshotUrls.length && (
                <p className="detail-desc" style={{ opacity: 0.55 }}>
                  Press F9 in-game to capture (when Aether is running).
                </p>
              )}
            </div>
          </div>
        </div>

        <aside className="detail-side">
          <div className="panel">
            <h3>Stats</h3>
            <div className="stat-grid">
              <div className="stat">
                <label>Playtime</label>
                <strong>{fmtPlaytime(game.playtimeMs)}</strong>
              </div>
              <div className="stat">
                <label>Launches</label>
                <strong>{game.launchCount}</strong>
              </div>
              <div className="stat">
                <label>Last played</label>
                <strong style={{ fontSize: 12 }}>{fmtDate(game.lastPlayed)}</strong>
              </div>
              <div className="stat">
                <label>Added</label>
                <strong style={{ fontSize: 12 }}>{fmtDate(game.addedAt)}</strong>
              </div>
            </div>
          </div>

          <div className="panel customize">
            <h3>Customize</h3>
            <div className="media-grid">
              <MediaTile
                label="Cover"
                url={game.coverUrl}
                kind={game.coverKind}
                tall
                onPick={() => pick("cover")}
              />
              <MediaTile
                label="Banner"
                url={game.bannerUrl}
                kind={game.bannerKind}
                onPick={() => pick("banner")}
              />
              <MediaTile
                label="Icon"
                url={game.iconUrl}
                kind="image"
                onPick={() => pick("icon")}
              />
            </div>

            <div className="form-grid">
              <label>
                Name
                <input
                  value={draft.name}
                  onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                  onBlur={save}
                />
              </label>
              <label>
                Category
                <input
                  value={draft.category}
                  onChange={(e) => setDraft({ ...draft, category: e.target.value })}
                  onBlur={save}
                />
              </label>
              <label>
                Description
                <textarea
                  rows={4}
                  value={draft.description}
                  onChange={(e) => setDraft({ ...draft, description: e.target.value })}
                  onBlur={save}
                  placeholder="Short blurb for this game…"
                />
              </label>
              <label>
                Tags
                <input
                  value={draft.tags}
                  onChange={(e) => setDraft({ ...draft, tags: e.target.value })}
                  onBlur={save}
                  placeholder="fps, multiplayer, …"
                />
              </label>
              <label>
                Launch args
                <input
                  value={draft.args}
                  onChange={(e) => setDraft({ ...draft, args: e.target.value })}
                  onBlur={save}
                  placeholder="-windowed …"
                />
              </label>
              <label>
                Accent (this game)
                <div style={{ display: "flex", gap: 10, alignItems: "center" }}>
                  <input
                    type="color"
                    value={draft.accent || "#0078f2"}
                    onChange={(e) => setDraft({ ...draft, accent: e.target.value })}
                    onBlur={save}
                  />
                  <button
                    type="button"
                    className="btn"
                    style={{ padding: "6px 10px" }}
                    onClick={() => {
                      setDraft({ ...draft, accent: "" });
                      api.updateGame(game.id, { accent: null }).then(onChange);
                    }}
                  >
                    Clear
                  </button>
                </div>
                <div className="swatches">
                  {ACCENT_SWATCHES.map((c) => (
                    <button
                      key={c}
                      type="button"
                      className={`swatch ${draft.accent?.toLowerCase() === c.toLowerCase() ? "active" : ""}`}
                      style={{ background: c }}
                      title={c}
                      onClick={() => {
                        setDraft({ ...draft, accent: c });
                        api.updateGame(game.id, { accent: c }).then(onChange);
                      }}
                    />
                  ))}
                </div>
              </label>
              <label>
                Executable
                <div className="exe-row">
                  <input readOnly value={game.exePath || ""} placeholder="No exe set" />
                  <button type="button" className="btn" onClick={pickExe}>
                    Browse…
                  </button>
                </div>
              </label>
            </div>

            <div className="detail-actions" style={{ marginTop: 14 }}>
              <button className="btn primary" disabled={saving} onClick={save}>
                {saving ? "Saving…" : "Save changes"}
              </button>
              <button
                className="btn danger"
                onClick={async () => {
                  if (!confirm(`Remove ${game.name} from Aether?`)) return;
                  await api.removeGame(game.id);
                  onBack();
                }}
              >
                Remove game
              </button>
            </div>
          </div>
        </aside>
      </div>

      {lightbox && (
        <div className="lightbox" onClick={() => setLightbox(null)}>
          <img src={lightbox} alt="" />
        </div>
      )}
    </div>
  );
}
