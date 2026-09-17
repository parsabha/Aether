import { open } from "@tauri-apps/plugin-dialog";
import { motion, AnimatePresence } from "motion/react";
import { useEffect, useState, type CSSProperties } from "react";
import { api } from "../../lib/api";
import type { Game, MediaKind } from "../../lib/types";
import { AutoPlayMedia } from "../media/AutoPlayMedia";
import { SessionTeaser } from "./SessionTeaser";

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

function ArtCard({
  label,
  hint,
  url,
  kind,
  ratio,
  onPick,
}: {
  label: string;
  hint: string;
  url?: string | null;
  kind?: MediaKind | string | null;
  ratio: "cover" | "banner" | "icon";
  onPick: () => void;
}) {
  return (
    <button type="button" className={`art-card art-${ratio}`} onClick={onPick}>
      <div className="art-card-frame">
        {url ? (
          <AutoPlayMedia src={url} kind={kind || "image"} className="art-card-media" />
        ) : (
          <div className="art-card-empty">
            <span className="art-card-plus">+</span>
            <span>Add {label.toLowerCase()}</span>
          </div>
        )}
      </div>
      <div className="art-card-meta">
        <strong>{label}</strong>
        <span>{url ? hint : "Click to browse"}</span>
      </div>
    </button>
  );
}

export function GameDetail({
  game,
  onBack,
  onChange,
  onLaunch,
  onOpenSessions,
  reduceMotion,
}: {
  game: Game;
  onBack: () => void;
  onChange: (g: Game) => void;
  onLaunch: () => void;
  onOpenSessions: () => void;
  reduceMotion?: boolean;
}) {
  const [lightbox, setLightbox] = useState<string | null>(null);
  const [editOpen, setEditOpen] = useState(false);
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

  useEffect(() => {
    if (!editOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setEditOpen(false);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [editOpen]);

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
      filters: [{ name: "Games", extensions: ["exe", "lnk", "bat", "cmd", "url"] }],
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
  const tags = (
    draft.tags
      ? draft.tags.split(",").map((t) => t.trim()).filter(Boolean)
      : game.tags
  );

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
            className="detail-hero-media detail-hero-fallback"
            style={{
              background: `linear-gradient(135deg, color-mix(in srgb, var(--accent) 48%, #1a1a28), #0a0a10)`,
            }}
          />
        )}
        <div className="detail-hero-scrim" />
        <div className="detail-hero-vignette" />

        <div className="detail-hero-top">
          <button type="button" className="back-btn" onClick={onBack}>
            <svg viewBox="0 0 24 24" aria-hidden>
              <path d="M15 6l-6 6 6 6" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            Library
          </button>
          <button
            type="button"
            className="detail-icon-btn"
            onClick={() => setEditOpen(true)}
            title="Game settings"
            aria-label="Game settings"
          >
            <svg viewBox="0 0 24 24" aria-hidden>
              <path
                d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.8"
              />
              <path
                d="M19.4 13a7.8 7.8 0 0 0 .05-2l2.05-1.6-2-3.46-2.45.8a7.7 7.7 0 0 0-1.73-1L14.9 3h-5.8l-.42 2.74a7.7 7.7 0 0 0-1.73 1l-2.45-.8-2 3.46L4.55 11a7.8 7.8 0 0 0 0 2l-2.05 1.6 2 3.46 2.45-.8a7.7 7.7 0 0 0 1.73 1L9.1 21h5.8l.42-2.74a7.7 7.7 0 0 0 1.73-1l2.45.8 2-3.46L19.4 13Z"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinejoin="round"
              />
            </svg>
          </button>
        </div>

        <div className="detail-hero-content">
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
              <p className="detail-kicker">{game.category || "Games"}</p>
              <h1>{draft.name || game.name}</h1>

              <div className="detail-meta-row">
                {game.running && <span className="detail-chip live">Playing now</span>}
                {game.missing && <span className="detail-chip warn">Missing exe</span>}
                <span className="detail-meta-stat">
                  <b>{fmtPlaytime(game.playtimeMs)}</b> played
                </span>
                <span className="detail-meta-dot" aria-hidden />
                <span className="detail-meta-stat">
                  <b>{game.launchCount}</b> launches
                </span>
                {game.lastPlayed ? (
                  <>
                    <span className="detail-meta-dot" aria-hidden />
                    <span className="detail-meta-stat">
                      Last <b>{fmtDate(game.lastPlayed)}</b>
                    </span>
                  </>
                ) : null}
              </div>

              {tags.length > 0 && (
                <div className="tag-row">
                  {tags.map((t) => (
                    <span key={t} className="tag">{t}</span>
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
                  className={`btn detail-ghost-btn ${game.favorite ? "accent" : ""}`}
                  onClick={async () =>
                    onChange(await api.updateGame(game.id, { favorite: !game.favorite }))
                  }
                >
                  {game.favorite ? "★ Favorited" : "☆ Favorite"}
                </button>
                {game.exePath && !game.exePath.toLowerCase().startsWith("steam://") && (
                  <button type="button" className="btn detail-ghost-btn" onClick={() => api.openFolder(game.exePath!)}>
                    Show in folder
                  </button>
                )}
                <button
                  type="button"
                  className="btn detail-ghost-btn"
                  onClick={() => setEditOpen(true)}
                >
                  Edit game
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="detail-body">
        <div className="detail-main">
          <section className="detail-section">
            <header className="detail-section-head">
              <h2>About</h2>
            </header>
            {draft.description || game.description ? (
              <p className="detail-desc">{draft.description || game.description}</p>
            ) : (
              <p className="detail-desc is-empty">
                No description yet. Open <button type="button" className="detail-inline-link" onClick={() => setEditOpen(true)}>Edit game</button> to add one.
              </p>
            )}
          </section>

          <section className="detail-section">
            <header className="detail-section-head">
              <h2>Artwork</h2>
              <button type="button" className="detail-section-action" onClick={() => setEditOpen(true)}>
                Manage
              </button>
            </header>
            <div className="art-grid">
              <ArtCard
                label="Cover"
                hint="Portrait art"
                url={game.coverUrl}
                kind={game.coverKind}
                ratio="cover"
                onPick={() => pick("cover")}
              />
              <ArtCard
                label="Banner"
                hint="Wide hero image"
                url={game.bannerUrl}
                kind={game.bannerKind}
                ratio="banner"
                onPick={() => pick("banner")}
              />
              <ArtCard
                label="Icon"
                hint="Small mark"
                url={game.iconUrl}
                kind="image"
                ratio="icon"
                onPick={() => pick("icon")}
              />
            </div>
          </section>

          <section className="detail-section">
            <header className="detail-section-head">
              <h2>Screenshots</h2>
              {game.screenshotUrls.length > 0 && (
                <span className="detail-section-count">{game.screenshotUrls.length}</span>
              )}
            </header>
            {game.screenshotUrls.length > 0 ? (
              <div className="shots-grid">
                {game.screenshotUrls.map((s) => (
                  <div key={s.path} className="shot-item">
                    <button type="button" className="shot-thumb" onClick={() => setLightbox(s.url)}>
                      <img src={s.url} alt="" />
                    </button>
                    <button
                      type="button"
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
              </div>
            ) : (
              <div className="shots-empty">
                <div className="shots-empty-icon" aria-hidden>
                  <svg viewBox="0 0 24 24">
                    <rect x="3" y="5" width="18" height="14" rx="2.5" fill="none" stroke="currentColor" strokeWidth="1.6" />
                    <circle cx="9" cy="11" r="1.6" fill="currentColor" />
                    <path d="M3 16l5-4 4 3 3-2 6 4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
                  </svg>
                </div>
                <p>No screenshots yet</p>
                <span>Press F9 in-game while Aether is running to capture.</span>
              </div>
            )}
          </section>

          <SessionTeaser gameId={game.id} onOpenResults={onOpenSessions} />
        </div>

        <aside className="detail-side">
          <section className="detail-section detail-stats">
            <header className="detail-section-head">
              <h2>Stats</h2>
            </header>
            <div className="stat-list">
              <div className="stat-row">
                <label>Playtime</label>
                <strong>{fmtPlaytime(game.playtimeMs)}</strong>
              </div>
              <div className="stat-row">
                <label>Launches</label>
                <strong>{game.launchCount}</strong>
              </div>
              <div className="stat-row">
                <label>Last played</label>
                <strong>{fmtDate(game.lastPlayed)}</strong>
              </div>
              <div className="stat-row">
                <label>Added</label>
                <strong>{fmtDate(game.addedAt)}</strong>
              </div>
              <div className="stat-row">
                <label>Category</label>
                <strong>{game.category || "Games"}</strong>
              </div>
            </div>
          </section>

          <section className="detail-section detail-quick">
            <header className="detail-section-head">
              <h2>Quick actions</h2>
            </header>
            <div className="quick-actions">
              <button type="button" className="quick-btn" onClick={() => setEditOpen(true)}>
                <span>Settings</span>
                <small>Name, media, accent</small>
              </button>
              {game.exePath && !game.exePath.toLowerCase().startsWith("steam://") && (
                <button type="button" className="quick-btn" onClick={() => api.openFolder(game.exePath!)}>
                  <span>Open folder</span>
                  <small>Reveal executable</small>
                </button>
              )}
              <button
                type="button"
                className="quick-btn danger"
                onClick={async () => {
                  if (!confirm(`Remove ${game.name} from Aether?`)) return;
                  await api.removeGame(game.id);
                  onBack();
                }}
              >
                <span>Remove game</span>
                <small>From your library</small>
              </button>
            </div>
          </section>
        </aside>
      </div>

      <AnimatePresence>
        {editOpen && (
          <motion.div
            key="game-edit"
            className="modal-backdrop detail-edit-backdrop"
            onClick={() => setEditOpen(false)}
            initial={reduceMotion ? false : { opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={reduceMotion ? undefined : { opacity: 0 }}
            transition={{ duration: 0.18 }}
          >
            <motion.div
              className="detail-edit-panel"
              onClick={(e) => e.stopPropagation()}
              initial={reduceMotion ? false : { opacity: 0, y: 18, scale: 0.98 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={reduceMotion ? undefined : { opacity: 0, y: 12, scale: 0.98 }}
              transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
            >
              <div className="settings-head">
                <div>
                  <h2>Game settings</h2>
                  <p className="settings-sub">Artwork, details, and launch options</p>
                </div>
                <button type="button" className="tb-btn" onClick={() => setEditOpen(false)} aria-label="Close">
                  ✕
                </button>
              </div>

              <section className="set-section">
                <h3>Artwork</h3>
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
              </section>

              <section className="set-section">
                <h3>Details</h3>
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
                </div>
              </section>

              <section className="set-section">
                <h3>Launch</h3>
                <div className="form-grid">
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
                    Executable
                    <div className="exe-row">
                      <input readOnly value={game.exePath || ""} placeholder="No exe set" />
                      <button type="button" className="btn" onClick={pickExe}>
                        Browse…
                      </button>
                    </div>
                  </label>
                </div>
              </section>

              <section className="set-section">
                <h3>Accent</h3>
                <div className="form-grid">
                  <label>
                    Color for this game
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
                </div>
              </section>

              <div className="detail-edit-footer">
                <button
                  type="button"
                  className="btn danger"
                  onClick={async () => {
                    if (!confirm(`Remove ${game.name} from Aether?`)) return;
                    await api.removeGame(game.id);
                    onBack();
                  }}
                >
                  Remove game
                </button>
                <div className="detail-edit-footer-right">
                  <button type="button" className="btn" onClick={() => setEditOpen(false)}>
                    Close
                  </button>
                  <button type="button" className="btn primary" disabled={saving} onClick={save}>
                    {saving ? "Saving…" : "Save changes"}
                  </button>
                </div>
              </div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>

      {lightbox && (
        <div className="lightbox" onClick={() => setLightbox(null)}>
          <img src={lightbox} alt="" />
        </div>
      )}
    </div>
  );
}
