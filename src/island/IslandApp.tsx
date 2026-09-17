import { motion } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import { applyChrome } from "../lib/chrome";
import type { Game, IslandMedia, IslandNotification, OverlayStats, Settings } from "../lib/types";

type IslandMode = "compact" | "peek" | "expanded";

function fmtClock(d: Date) {
  return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function fmtDay(d: Date) {
  return d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
}

function fmtDuration(ms: number) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}:${r.toString().padStart(2, "0")}`;
}

function fmtMetric(n: number | null | undefined, digits = 0) {
  if (n == null || !Number.isFinite(n)) return "—";
  return digits > 0 ? n.toFixed(digits) : Math.round(n).toString();
}

function fmtVram(used?: number | null, total?: number | null) {
  if (used == null || !Number.isFinite(used)) return "—";
  const u = used >= 1024 ? `${(used / 1024).toFixed(1)}G` : `${Math.round(used)}M`;
  if (total == null || !Number.isFinite(total) || total <= 0) return u;
  const t = total >= 1024 ? `${(total / 1024).toFixed(0)}G` : `${Math.round(total)}M`;
  return `${u}/${t}`;
}

function gameArt(game?: Game) {
  return game?.iconUrl || game?.coverUrl || null;
}

/** SF-Symbols-flavoured glyphs, drawn on a 24px grid with a 1.7 stroke. */
const glyph = {
  bell: (
    <path d="M12 4.2a4.6 4.6 0 0 0-4.6 4.6v2.6c0 1.1-.4 2.1-1.1 2.9l-.2.2a.75.75 0 0 0 .55 1.25h10.6a.75.75 0 0 0 .55-1.25l-.2-.2a4.3 4.3 0 0 1-1.1-2.9V8.8A4.6 4.6 0 0 0 12 4.2Z" />
  ),
  bellClapper: <path d="M10.1 18.1a2 2 0 0 0 3.8 0" />,
  camera: (
    <>
      <path d="M4.6 9.1c0-1.05.85-1.9 1.9-1.9h1.7l.95-1.7h5.7l.95 1.7h1.7c1.05 0 1.9.85 1.9 1.9v7.2c0 1.05-.85 1.9-1.9 1.9H6.5a1.9 1.9 0 0 1-1.9-1.9V9.1Z" />
      <circle cx="12" cy="12.7" r="3.1" />
    </>
  ),
  timer: (
    <>
      <circle cx="12" cy="12.6" r="7.4" />
      <path d="M12 8.4v4.3l2.9 1.7" />
    </>
  ),
  note: (
    <>
      <path d="M9.4 16.6V6.9l7.4-1.6v9.4" />
      <circle cx="7.3" cy="17.1" r="2.2" />
      <circle cx="14.7" cy="15.4" r="2.2" />
    </>
  ),
  dot: <circle cx="12" cy="12" r="3.2" />,
};

function NotifIcon({ kind }: { kind: string }) {
  const paths =
    kind === "screenshot"
      ? glyph.camera
      : kind === "session"
        ? glyph.timer
        : kind === "system"
          ? (
              <>
                {glyph.bell}
                {glyph.bellClapper}
              </>
            )
          : glyph.dot;

  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7"
      strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      {paths}
    </svg>
  );
}

const IconNote = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7"
    strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    {glyph.note}
  </svg>
);

const IconPlay = () => (
  <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden>
    <path d="M8.4 5.6a1 1 0 0 1 1.5-.87l8.2 5.4a1 1 0 0 1 0 1.74l-8.2 5.4a1 1 0 0 1-1.5-.87V5.6Z" />
  </svg>
);

const IconPause = () => (
  <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden>
    <rect x="7.6" y="5.4" width="3.2" height="13.2" rx="1.4" />
    <rect x="13.2" y="5.4" width="3.2" height="13.2" rx="1.4" />
  </svg>
);

const IconSkip = ({ back = false }: { back?: boolean }) => (
  <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden
    style={back ? { transform: "scaleX(-1)" } : undefined}>
    <path d="M6.6 6.3a.9.9 0 0 1 1.36-.78l7 4.9a.9.9 0 0 1 0 1.5l-7 4.9a.9.9 0 0 1-1.36-.78V6.3Z" />
    <rect x="15.9" y="5.5" width="2.5" height="13" rx="1.25" />
  </svg>
);

const SPRING_OPEN = { type: "spring", visualDuration: 0.42, bounce: 0.26 } as const;
const SPRING_CLOSE = { type: "spring", visualDuration: 0.32, bounce: 0.1 } as const;
const SPRING_PEEK = { type: "spring", visualDuration: 0.34, bounce: 0.34 } as const;

/// Tracks the natural (content) size of an element so the pill can animate to it.
function useMeasure<T extends HTMLElement>() {
  const ref = useRef<T | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const read = () => {
      const w = el.offsetWidth;
      const h = el.offsetHeight;
      setSize((prev) =>
        Math.abs(prev.width - w) < 1 && Math.abs(prev.height - h) < 1
          ? prev
          : { width: w, height: h },
      );
    };
    read();
    const ro = new ResizeObserver(read);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return { ref, ...size };
}

export function IslandApp() {
  const [games, setGames] = useState<Game[]>([]);
  const [media, setMedia] = useState<IslandMedia | null>(null);
  const [notifications, setNotifications] = useState<IslandNotification[]>([]);
  const [expanded, setExpanded] = useState(false);
  const [query, setQuery] = useState("");
  const [clock, setClock] = useState(() => new Date());
  const [peekUntil, setPeekUntil] = useState(0);
  const [reduceMotion, setReduceMotion] = useState(false);
  const [systemAccess, setSystemAccess] = useState(true);
  const [overlayEnabled, setOverlayEnabled] = useState(false);
  const [overlayFlags, setOverlayFlags] = useState({
    fps: true,
    cpuUsage: true,
    gpuUsage: true,
    cpuTemp: true,
    vram: true,
  });
  const [stats, setStats] = useState<OverlayStats>({});
  const collapseTimer = useRef<number | undefined>(undefined);
  const openTimer = useRef<number | undefined>(undefined);
  const prevNotifId = useRef<string | null>(null);
  const prevTrack = useRef<string | null>(null);
  const lastLayout = useRef({ w: 0, h: 0 });

  const applyOverlaySettings = useCallback((s: Settings) => {
    setReduceMotion(!!s.reduceMotion);
    setOverlayEnabled(!!s.overlayEnabled);
    setOverlayFlags({
      fps: s.overlayShowFps !== false,
      cpuUsage: s.overlayShowCpuUsage !== false,
      gpuUsage: s.overlayShowGpuUsage !== false,
      cpuTemp: s.overlayShowCpuTemp !== false,
      vram: s.overlayShowVram !== false,
    });
    applyChrome(s);
  }, []);

  const loadGames = useCallback(async () => {
    const [g, s] = await Promise.all([api.islandGames(), api.getSettings()]);
    setGames(g);
    applyOverlaySettings(s);
  }, [applyOverlaySettings]);

  const loadFeed = useCallback(async () => {
    try {
      const feed = await api.islandFeed();
      setMedia(feed.media || null);
      setNotifications(feed.notifications || []);
      setSystemAccess(!!feed.systemAccess);
    } catch {
      /* island feed optional */
    }
  }, []);

  // A new toast or a track change gives the pill a brief, wider preview.
  const peek = useCallback((ms = 4200) => setPeekUntil(Date.now() + ms), []);

  const loadBusy = useRef(false);
  const gamesBusy = useRef(false);
  const load = useCallback(async () => {
    if (loadBusy.current) return;
    loadBusy.current = true;
    try {
      await loadFeed();
    } finally {
      loadBusy.current = false;
    }
  }, [loadFeed]);

  const refreshGames = useCallback(async () => {
    if (gamesBusy.current) return;
    gamesBusy.current = true;
    try {
      await loadGames();
    } finally {
      gamesBusy.current = false;
    }
  }, [loadGames]);

  useEffect(() => {
    document.documentElement.classList.add("is-island");
    document.body.classList.add("is-island");
    refreshGames();
    load();
    let unLib: (() => void) | undefined;
    let unFeed: (() => void) | undefined;
    let unSet: (() => void) | undefined;
    api.onLibraryChanged(() => refreshGames()).then((u) => {
      unLib = u;
    });
    api.onSettingsChanged((s) => {
      applyOverlaySettings(s);
    }).then((u) => {
      unSet = u;
    });
    api.onIslandFeed(() => {
      // Skip feed work while the in-game HUD is up — keeps the WebView quiet.
      if (document.documentElement.classList.contains("is-overlay-hud")) return;
      loadFeed();
      peek();
    }).then((u) => {
      unFeed = u;
    });
    return () => {
      document.documentElement.classList.remove("is-island");
      document.body.classList.remove("is-island");
      document.documentElement.classList.remove("is-overlay-hud");
      unLib?.();
      unFeed?.();
      unSet?.();
    };
  }, [load, loadFeed, refreshGames, peek, applyOverlaySettings]);

  useEffect(() => {
    if (peekUntil <= Date.now()) return;
    const delay = peekUntil - Date.now() + 40;
    const t = window.setTimeout(() => setPeekUntil(0), delay);
    return () => clearTimeout(t);
  }, [peekUntil]);

  const running = games.filter((g) => g.running);
  const nowGame = running[0] || games[0];
  const unread = notifications.filter((n) => !n.read);
  const latestUnread = unread[0];
  const showMedia = media && media.title && media.status !== "stopped";
  const overlayActive = overlayEnabled && running.length > 0;

  // Desktop island timers — paused entirely during in-game overlay HUD.
  useEffect(() => {
    if (overlayActive) {
      document.documentElement.classList.add("is-overlay-hud");
      return () => document.documentElement.classList.remove("is-overlay-hud");
    }
    document.documentElement.classList.remove("is-overlay-hud");
    const t = window.setInterval(load, 5000);
    const gamesT = window.setInterval(refreshGames, 30_000);
    const clockT = window.setInterval(() => setClock(new Date()), 30_000);
    return () => {
      clearInterval(t);
      clearInterval(gamesT);
      clearInterval(clockT);
    };
  }, [overlayActive, load, refreshGames]);

  useEffect(() => {
    if (!overlayActive) return;
    setExpanded(false);
    setQuery("");
    setPeekUntil(0);
  }, [overlayActive]);

  useEffect(() => {
    if (!overlayActive) {
      setStats({});
      return;
    }
    let un: (() => void) | undefined;
    api.overlayStats()
      .then(setStats)
      .catch(() => {});
    api.onOverlayStats(setStats).then((u) => {
      un = u;
    });
    return () => un?.();
  }, [overlayActive]);

  const overlayItems = useMemo(() => {
    if (!overlayActive) return [];
    const items: { key: string; label: string; value: string }[] = [];
    if (overlayFlags.fps) {
      items.push({ key: "fps", label: "FPS", value: fmtMetric(stats.fps) });
    }
    if (overlayFlags.cpuUsage) {
      items.push({ key: "cpu", label: "CPU", value: `${fmtMetric(stats.cpuUsage)}%` });
    }
    if (overlayFlags.gpuUsage) {
      items.push({ key: "gpu", label: "GPU", value: `${fmtMetric(stats.gpuUsage)}%` });
    }
    if (overlayFlags.cpuTemp) {
      items.push({ key: "temp", label: "TEMP", value: `${fmtMetric(stats.cpuTempC)}°` });
    }
    if (overlayFlags.vram) {
      items.push({
        key: "vram",
        label: "VRAM",
        value: fmtVram(stats.vramUsedMb, stats.vramTotalMb),
      });
    }
    return items;
  }, [overlayActive, overlayFlags, stats]);

  useEffect(() => {
    const newest = unread[0]?.id;
    if (newest && newest !== prevNotifId.current) {
      prevNotifId.current = newest;
      peek();
    }
    if (!newest) prevNotifId.current = null;
  }, [unread, peek]);

  // Track changes get their own quick preview, like iOS.
  useEffect(() => {
    const key = showMedia ? `${media!.title}|${media!.artist}` : "";
    if (key && key !== prevTrack.current && media!.status === "playing") {
      if (prevTrack.current !== null) peek(3200);
      prevTrack.current = key;
    } else if (!key) {
      prevTrack.current = null;
    }
  }, [showMedia, media, peek]);

  const peekActive = !expanded && Date.now() < peekUntil;
  const mode: IslandMode = expanded ? "expanded" : peekActive ? "peek" : "compact";
  const peekNotif = latestUnread && Date.now() - latestUnread.createdAt < 30_000
    ? latestUnread
    : null;

  const liveKind = showMedia && media!.status === "playing"
    ? "media"
    : running.length > 0
      ? "game"
      : null;
  const compactTitle = showMedia
    ? media!.title
    : running.length > 0
      ? nowGame?.name || ""
      : "";

  // Both layers stay mounted and overlap, so the pill can be sized from their
  // real measured content instead of hard-coded guesses.
  const strip = useMeasure<HTMLDivElement>();
  const body = useMeasure<HTMLDivElement>();

  const pillW =
    mode === "expanded"
      ? 372
      : overlayActive && mode === "compact"
        ? Math.max(248, Math.min(520, strip.width || 280))
        : Math.max(208, Math.min(400, strip.width || 208));
  const pillH =
    mode === "expanded" ? Math.max(180, body.height || 280) : Math.max(40, strip.height || 40);

  useEffect(() => {
    const w = Math.round(pillW);
    const h = Math.round(pillH);
    // In-game HUD: ignore micro size jitter from changing FPS digits — each
    // SetWindowPos hitchs DWM-composed Vulkan titles.
    if (
      overlayActive &&
      Math.abs(w - lastLayout.current.w) < 12 &&
      Math.abs(h - lastLayout.current.h) < 6 &&
      lastLayout.current.w > 0
    ) {
      return;
    }
    lastLayout.current = { w, h };
    api.islandLayout(w, h).catch(() => {});
  }, [pillW, pillH, overlayActive]);

  useEffect(() => {
    if (expanded && unread.length) {
      api.islandMarkNotificationsRead().catch(() => {});
      setNotifications((prev) => prev.map((n) => ({ ...n, read: true })));
    }
  }, [expanded, unread.length]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return games;
    return games.filter((g) => g.name.toLowerCase().includes(q));
  }, [games, query]);

  const onEnter = () => {
    if (overlayActive) return;
    if (collapseTimer.current) window.clearTimeout(collapseTimer.current);
    if (openTimer.current) window.clearTimeout(openTimer.current);
    openTimer.current = window.setTimeout(() => setExpanded(true), 70);
  };
  const onLeave = () => {
    if (overlayActive) return;
    if (openTimer.current) window.clearTimeout(openTimer.current);
    collapseTimer.current = window.setTimeout(() => {
      setExpanded(false);
      setQuery("");
    }, 200);
  };

  const dismissNotif = (id: string) => {
    api.islandDismissNotification(id).catch(() => {});
    setNotifications((prev) =>
      prev.map((n) => (n.id === id ? { ...n, read: true } : n)),
    );
  };

  const notifGame = (n: IslandNotification) =>
    n.gameId ? games.find((g) => g.id === n.gameId) : undefined;

  return (
    <div className="island-root">
      <motion.div
        className={[
          "island-pill",
          mode === "expanded" ? "is-open" : "",
          mode === "peek" ? "is-peek" : "",
          running.length ? "is-live" : "",
          showMedia ? "is-media" : "",
          overlayActive && mode === "compact" ? "is-overlay" : "",
        ]
          .filter(Boolean)
          .join(" ")}
        onMouseEnter={onEnter}
        onMouseLeave={onLeave}
        style={overlayActive ? { pointerEvents: "none" } : undefined}
        initial={false}
        animate={{
          width: pillW,
          height: pillH,
          borderRadius: mode === "expanded" ? 38 : 22,
        }}
        transition={
          reduceMotion
            ? { duration: 0.12 }
            : mode === "expanded"
              ? SPRING_OPEN
              : mode === "peek"
                ? SPRING_PEEK
                : SPRING_CLOSE
        }
      >
        {/* —— Compact / peek strip —— */}
        <motion.div
          className="island-layer island-strip"
          initial={false}
          animate={{
            opacity: mode === "expanded" ? 0 : 1,
            scale: mode === "expanded" ? 0.9 : 1,
            y: mode === "expanded" ? -6 : 0,
          }}
          transition={{
            duration: mode === "expanded" ? 0.11 : 0.2,
            ease: mode === "expanded" ? [0.4, 0, 1, 1] : [0.16, 1, 0.3, 1],
            delay: mode === "expanded" ? 0 : 0.04,
          }}
          style={{ pointerEvents: mode === "expanded" ? "none" : "auto" }}
        >
          <div className="island-strip-inner" ref={strip.ref}>
            {mode === "peek" && peekNotif ? (
              <button type="button" className="island-peek" onClick={() => api.showMain()}>
                <span className="island-peek-ico">
                  <NotifIcon kind={peekNotif.kind} />
                </span>
                <span className="island-peek-line">
                  <strong>{peekNotif.title}</strong>
                  <em>{peekNotif.body}</em>
                </span>
                {unread.length > 1 && <span className="island-badge sm">{unread.length}</span>}
              </button>
            ) : mode === "peek" && showMedia ? (
              <div className="island-peek">
                {media!.thumbnailDataUrl ? (
                  <img className="island-thumb" src={media!.thumbnailDataUrl} alt="" />
                ) : (
                  <span className="island-thumb island-thumb-empty">
                    <IconNote />
                  </span>
                )}
                <span className="island-peek-line">
                  <strong>{media!.title}</strong>
                  <em>{media!.artist || media!.appName || "Now playing"}</em>
                </span>
                {media!.status === "playing" && (
                  <span className="eq" aria-hidden>
                    <i /><i /><i />
                  </span>
                )}
              </div>
            ) : overlayActive ? (
              <div className="island-compact island-overlay">
                {gameArt(nowGame) ? (
                  <img className="island-lens" src={gameArt(nowGame)!} alt="" />
                ) : (
                  <span className="island-lens island-lens-empty" />
                )}
                <div className="island-overlay-metrics">
                  {overlayItems.map((item) => (
                    <span key={item.key} className="island-metric">
                      <em>{item.label}</em>
                      <strong>{item.value}</strong>
                    </span>
                  ))}
                </div>
              </div>
            ) : (
              <div className="island-compact">
                {showMedia && media!.thumbnailDataUrl ? (
                  <img className="island-lens" src={media!.thumbnailDataUrl} alt="" />
                ) : showMedia ? (
                  <span className="island-lens island-lens-note">
                    <IconNote />
                  </span>
                ) : gameArt(nowGame) ? (
                  <img className="island-lens" src={gameArt(nowGame)!} alt="" />
                ) : (
                  <span className="island-lens island-lens-empty" />
                )}

                <span className="island-compact-mid">
                  {compactTitle ? (
                    <span className="island-compact-title">{compactTitle}</span>
                  ) : (
                    <span className="island-compact-day">{fmtDay(clock)}</span>
                  )}
                </span>

                <span className="island-compact-tail">
                  {unread.length > 0 && <span className="island-badge sm">{unread.length}</span>}
                  {liveKind && (
                    <span className="eq" aria-hidden>
                      <i /><i /><i />
                    </span>
                  )}
                  <span className="island-compact-time">{fmtClock(clock)}</span>
                </span>
              </div>
            )}
          </div>
        </motion.div>

        {/* —— Expanded panel —— */}
        <motion.div
          className="island-layer island-body-layer"
          initial={false}
          animate={{
            opacity: mode === "expanded" ? 1 : 0,
            scale: mode === "expanded" ? 1 : 0.93,
            y: mode === "expanded" ? 0 : -8,
          }}
          transition={{
            duration: mode === "expanded" ? 0.28 : 0.1,
            delay: mode === "expanded" ? 0.07 : 0,
            ease: mode === "expanded" ? [0.16, 1, 0.3, 1] : [0.4, 0, 1, 1],
          }}
          style={{ pointerEvents: mode === "expanded" ? "auto" : "none" }}
        >
            <div className="island-body" ref={body.ref}>
              {showMedia && (
                <div className="island-media-card">
                  {media!.thumbnailDataUrl ? (
                    <img src={media!.thumbnailDataUrl} alt="" />
                  ) : (
                    <span className="island-np-ph island-np-note">
                      <IconNote />
                    </span>
                  )}
                  <div className="island-np-meta">
                    <strong>{media!.title}</strong>
                    <span>
                      {media!.artist || media!.album || media!.appName || "Now playing"}
                    </span>
                    {media!.durationMs > 0 ? (
                      <>
                        <span className="island-media-bar">
                          <i
                            style={{
                              width: `${Math.min(100, (media!.positionMs / media!.durationMs) * 100)}%`,
                            }}
                          />
                        </span>
                        <span className="island-media-time">
                          {fmtDuration(media!.positionMs)} / {fmtDuration(media!.durationMs)}
                        </span>
                      </>
                    ) : (
                      <span className="island-media-time">
                        {media!.status === "playing" ? "Playing" : "Paused"}
                      </span>
                    )}
                  </div>
                  <div className="island-media-controls">
                    <button type="button" onClick={() => api.islandMediaPrevious()} aria-label="Previous">
                      <IconSkip back />
                    </button>
                    <button type="button" onClick={() => api.islandMediaToggle()} aria-label="Play/Pause">
                      {media!.status === "playing" ? <IconPause /> : <IconPlay />}
                    </button>
                    <button type="button" onClick={() => api.islandMediaNext()} aria-label="Next">
                      <IconSkip />
                    </button>
                  </div>
                </div>
              )}

              <button type="button" className="island-nowplaying" onClick={() => api.showMain()}>
                {gameArt(nowGame) ? (
                  <img src={gameArt(nowGame)!} alt="" />
                ) : (
                  <span className="island-np-ph">{(nowGame?.name || "A").slice(0, 1)}</span>
                )}
                <div className="island-np-meta">
                  <strong>{nowGame?.name || "Aether"}</strong>
                  <span>
                    {running.length
                      ? "Playing now"
                      : showMedia
                        ? "Library · music in background"
                        : "Library"}
                  </span>
                </div>
                {nowGame && !nowGame.missing && (
                  <span
                    className="island-play"
                    onClick={(e) => {
                      e.stopPropagation();
                      api.launchGame(nowGame.id).then(load);
                    }}
                  >
                    <IconPlay />
                  </span>
                )}
              </button>

              {(notifications.length > 0 || !systemAccess) && (
                <div className="island-notifs">
                  <div className="island-notifs-head">
                    <span>Notifications</span>
                    <button
                      type="button"
                      className="island-notifs-clear"
                      onClick={() => {
                        api.islandClearNotifications().catch(() => {});
                        setNotifications([]);
                      }}
                    >
                      Clear
                    </button>
                  </div>
                  {notifications.slice(0, 4).map((n) => {
                    const g = notifGame(n);
                    return (
                      <button
                        key={n.id}
                        type="button"
                        className={`island-notif ${n.read ? "read" : ""}`}
                        onClick={() => {
                          dismissNotif(n.id);
                          if (n.kind !== "system") api.showMain();
                        }}
                      >
                        {g && gameArt(g) ? (
                          <img src={gameArt(g)!} alt="" />
                        ) : (
                          <span className="island-notif-ico">
                            <NotifIcon kind={n.kind} />
                          </span>
                        )}
                        <span className="island-notif-text">
                          <strong>{n.title}</strong>
                          <span>{n.body}</span>
                        </span>
                        {n.kind === "system" && n.app && (
                          <span className="island-notif-app">{n.app}</span>
                        )}
                      </button>
                    );
                  })}
                  {!systemAccess && (
                    <p className="island-notifs-hint">
                      Windows blocked notification access, so only Aether events show here.
                    </p>
                  )}
                </div>
              )}

              <input
                className="island-search"
                placeholder="Search games"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />

              <div className="island-apps">
                {filtered.slice(0, 6).map((g) => (
                  <button
                    key={g.id}
                    type="button"
                    className={`island-app ${g.running ? "live" : ""}`}
                    disabled={!!g.missing}
                    title={g.name}
                    onClick={() => api.launchGame(g.id).then(load)}
                  >
                    {g.iconUrl || g.coverUrl ? (
                      <img src={g.iconUrl || g.coverUrl!} alt="" />
                    ) : (
                      <span>{g.name.slice(0, 1)}</span>
                    )}
                  </button>
                ))}
                {!filtered.length && <p className="island-empty">No games</p>}
              </div>
            </div>
        </motion.div>
      </motion.div>
    </div>
  );
}
