import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Sidebar, TitleBar } from "./components/chrome";
import { GameDetail } from "./components/game/GameDetail";
import { SessionResultsPage } from "./components/game/SessionResultsPage";
import { SettingsPanel } from "./components/game/SettingsPanel";
import { GameCard } from "./components/library/GameCard";
import { VirtualGrid } from "./components/library/VirtualGrid";
import { AutoPlayMedia } from "./components/media/AutoPlayMedia";
import { IslandApp } from "./island/IslandApp";
import { applyChrome } from "./lib/chrome";
import { api } from "./lib/api";
import type { Game, Settings } from "./lib/types";
import { useCustomOrder } from "./lib/useCustomOrder";

function isIslandRoute() {
  try {
    if (getCurrentWindow().label === "island") return true;
  } catch {
    /* ignore */
  }
  return window.location.hash === "#island";
}

export default function App() {
  const [island] = useState(isIslandRoute);
  if (island) return <IslandApp />;
  return <MainApp />;
}

function MainApp() {
  const [games, setGames] = useState<Game[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [filter, setFilter] = useState("all");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [sessionsForId, setSessionsForId] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [bootError, setBootError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);

  const refresh = useCallback(async () => {
    const [g, s] = await Promise.all([api.getLibrary(), api.getSettings()]);
    setGames(g);
    setSettings(s);
    setBootError(null);
    applyChrome(s);
  }, []);

  useEffect(() => {
    refresh().catch((e) => {
      console.error(e);
      setBootError(e instanceof Error ? e.message : String(e));
    });
    const onRefresh = () => refresh();
    window.addEventListener("aether:refresh", onRefresh);
    let u1: (() => void) | undefined;
    let u2: (() => void) | undefined;
    api.onLibraryChanged(() => refresh()).then((u) => {
      u1 = u;
    });
    api.onSettingsChanged((s) => {
      setSettings(s);
      applyChrome(s);
    }).then((u) => {
      u2 = u;
    });
    api.onScreenshot(() => {
      setToast("Screenshot saved");
      refresh();
    });
    let dropUn: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent(async (event) => {
        if (event.payload.type !== "drop") return;
        const paths = event.payload.paths.filter((p) =>
          /\.(exe|lnk|bat|cmd)$/i.test(p),
        );
        if (!paths.length) return;
        await api.addGames(paths);
        setToast(`Added ${paths.length} game${paths.length === 1 ? "" : "s"}`);
        refresh();
      })
      .then((u) => {
        dropUn = u;
      });
    return () => {
      window.removeEventListener("aether:refresh", onRefresh);
      u1?.();
      u2?.();
      dropUn?.();
    };
  }, [refresh]);

  useEffect(() => {
    if (!toast) return;
    const t = window.setTimeout(() => setToast(null), 2400);
    return () => clearTimeout(t);
  }, [toast]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      const typing = tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
      if (e.key === "/" && !typing) {
        e.preventDefault();
        searchRef.current?.focus();
      }
      if (e.key === "Escape") {
        if (showSettings) setShowSettings(false);
        else if (sessionsForId) setSessionsForId(null);
        else if (selectedId) setSelectedId(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [showSettings, selectedId, sessionsForId]);

  const categories = useMemo(() => {
    const set = new Set(games.map((g) => g.category || "Games"));
    return [...set].sort();
  }, [games]);

  const counts = useMemo(
    () => ({
      all: games.length,
      favorite: games.filter((g) => g.favorite).length,
      running: games.filter((g) => g.running).length,
    }),
    [games],
  );

  const canReorder =
    !!settings &&
    settings.sort === "custom" &&
    filter === "all" &&
    !query.trim() &&
    !selectedId;

  const filtered = useMemo(() => {
    let list = [...games];
    if (filter === "favorite") list = list.filter((g) => g.favorite);
    else if (filter === "running") list = list.filter((g) => g.running);
    else if (filter === "recent") {
      list.sort((a, b) => (b.lastPlayed || 0) - (a.lastPlayed || 0));
      list = list.filter((g) => g.lastPlayed);
    } else if (filter.startsWith("cat:")) {
      const c = filter.slice(4);
      list = list.filter((g) => g.category === c);
    }
    const q = query.trim().toLowerCase();
    if (q) {
      list = list.filter(
        (g) =>
          g.name.toLowerCase().includes(q) ||
          g.tags.some((t) => t.toLowerCase().includes(q)) ||
          g.category.toLowerCase().includes(q),
      );
    }
    const sort = settings?.sort || "custom";
    if (sort === "name") list.sort((a, b) => a.name.localeCompare(b.name));
    else if (sort === "lastPlayed") list.sort((a, b) => (b.lastPlayed || 0) - (a.lastPlayed || 0));
    else if (sort === "playtime") list.sort((a, b) => b.playtimeMs - a.playtimeMs);
    else if (sort === "added") list.sort((a, b) => b.addedAt - a.addedAt);
    else list.sort((a, b) => a.sortOrder - b.sortOrder);
    return list;
  }, [games, filter, query, settings?.sort]);

  const { dragId, ghost, onCardPointerDown, didDrag } = useCustomOrder(
    canReorder,
    scrollRef,
    games,
    setGames,
  );

  const selected = games.find((g) => g.id === selectedId) || null;
  const sessionsGame = games.find((g) => g.id === sessionsForId) || null;

  const addGames = async () => {
    const selected = await open({
      multiple: true,
      filters: [{ name: "Games", extensions: ["exe", "lnk", "bat", "cmd", "url"] }],
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    await api.addGames(paths);
    setToast(`Added ${paths.length} game${paths.length === 1 ? "" : "s"}`);
    refresh();
  };

  if (!settings) {
    return (
      <div className="boot">
        {bootError ? (
          <>
            <div style={{ maxWidth: 420, textAlign: "center", lineHeight: 1.5 }}>
              <div style={{ color: "#fff", marginBottom: 8, fontWeight: 700 }}>Failed to start</div>
              <div style={{ color: "var(--text-dim)", fontSize: 13, marginBottom: 14 }}>{bootError}</div>
              <button className="btn primary" onClick={() => { setBootError(null); refresh().catch((e) => setBootError(String(e))); }}>
                Retry
              </button>
            </div>
          </>
        ) : (
          "Loading Aether…"
        )}
      </div>
    );
  }

  return (
    <div className={`app-shell ${settings.reduceMotion ? "reduce-motion" : ""}`}>
      <TitleBar
        islandOn={settings.islandEnabled}
        onToggleIsland={async () => {
          const on = await api.toggleIsland();
          setSettings({ ...settings, islandEnabled: on });
        }}
      />
      <div className="layout">
        <Sidebar
          filter={filter}
          setFilter={setFilter}
          categories={categories}
          counts={counts}
          onAdd={addGames}
          onSettings={() => setShowSettings(true)}
        />
        <main
          className="content"
          ref={(el) => {
            scrollRef.current = el;
          }}
        >
          {!selected && !sessionsGame && (
            <>
              <div className="topbar">
                <div className="search">
                  <input
                    ref={searchRef}
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    placeholder="Search library…  (/)"
                    spellCheck={false}
                  />
                </div>
                <select
                  className="select"
                  value={settings.sort}
                  onChange={async (e) => {
                    const s = await api.setSettings({ sort: e.target.value });
                    setSettings(s);
                  }}
                >
                  <option value="custom">Custom Order</option>
                  <option value="lastPlayed">Recently Played</option>
                  <option value="name">Name (A–Z)</option>
                  <option value="playtime">Most Played</option>
                  <option value="added">Recently Added</option>
                </select>
                <div className="view-toggle">
                  <button
                    className={settings.view === "grid" ? "active" : ""}
                    onClick={() => api.setSettings({ view: "grid" }).then(setSettings)}
                  >
                    Grid
                  </button>
                  <button
                    className={settings.view === "list" ? "active" : ""}
                    onClick={() => api.setSettings({ view: "list" }).then(setSettings)}
                  >
                    List
                  </button>
                </div>
              </div>
              {filtered.length > 0 && (
                <div className="page-head">
                  <div>
                    <h1>
                      {filter === "all"
                        ? "Library"
                        : filter === "favorite"
                          ? "Favorites"
                          : filter === "recent"
                            ? "Recently played"
                            : filter === "running"
                              ? "Playing"
                              : filter.startsWith("cat:")
                                ? filter.slice(4)
                                : "Library"}
                    </h1>
                    <p>{filtered.length} {filtered.length === 1 ? "game" : "games"}</p>
                  </div>
                </div>
              )}

              {filtered.length > 0 && settings.view === "grid" && (
                <VirtualGrid
                  items={filtered}
                  scrollRef={scrollRef}
                  minCardWidth={190}
                  gap={18}
                  aspect={4 / 3}
                  metaH={0}
                  overscan={2}
                  getKey={(g) => g.id}
                  renderItem={(g) => (
                    <GameCard
                      game={g}
                      onOpen={() => setSelectedId(g.id)}
                      onLaunch={() => api.launchGame(g.id).then(refresh)}
                      canDrag={canReorder}
                      dragging={dragId === g.id}
                      reduceMotion={settings.reduceMotion}
                      onPointerDown={(e) => onCardPointerDown(g.id, e)}
                      suppressOpen={didDrag}
                    />
                  )}
                />
              )}

              {filtered.length > 0 && settings.view === "list" && (
                <div className="list-view">
                  {filtered.map((g) => (
                    <div
                      key={g.id}
                      data-gid={g.id}
                      role="button"
                      tabIndex={0}
                      className={`list-row ${canReorder ? "can-drag" : ""} ${dragId === g.id ? "is-dragging" : ""}`}
                      onPointerDown={canReorder ? (e) => onCardPointerDown(g.id, e) : undefined}
                      onClick={() => {
                        if (didDrag()) return;
                        setSelectedId(g.id);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") setSelectedId(g.id);
                      }}
                      onDoubleClick={() => !g.missing && api.launchGame(g.id).then(refresh)}
                    >
                      <div className="list-art">
                        {g.coverUrl ? (
                          <AutoPlayMedia
                            src={g.coverUrl}
                            kind={g.coverKind}
                            poster={g.coverPoster}
                            className="list-art-media"
                          />
                        ) : (
                          <span>{g.name.slice(0, 1)}</span>
                        )}
                      </div>
                      <div className="list-meta">
                        <strong>{g.name}</strong>
                        <span>{g.category}{g.favorite ? " · ★" : ""}</span>
                      </div>
                      {g.running && <span className="run-dot" />}
                    </div>
                  ))}
                </div>
              )}

              {filtered.length === 0 && (
                <div className="empty">
                  <div className="empty-mark" aria-hidden>
                    <span>+</span>
                  </div>
                  {games.length === 0 ? (
                    <>
                      <h2>Your library is empty</h2>
                      <p>Drop a game .exe here, or browse for titles to add.</p>
                      <button type="button" className="add-btn add-btn-lg" onClick={addGames}>
                        <span className="add-ico" aria-hidden>
                          <svg viewBox="0 0 24 24" fill="none">
                            <path d="M12 5v14M5 12h14" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" />
                          </svg>
                        </span>
                        <span className="add-text">
                          Add Games
                          <small>Browse for .exe, shortcut, or Steam .url</small>
                        </span>
                      </button>
                    </>
                  ) : (
                    <>
                      <h2>Nothing here</h2>
                      <p>No games match this view. Try another filter or search.</p>
                      <button className="btn primary" onClick={() => { setFilter("all"); setQuery(""); }}>
                        Show all games
                      </button>
                    </>
                  )}
                </div>
              )}
            </>
          )}

          {sessionsGame ? (
            <SessionResultsPage
              game={sessionsGame}
              onBack={() => setSessionsForId(null)}
            />
          ) : selected ? (
            <GameDetail
              game={selected}
              reduceMotion={settings.reduceMotion}
              onBack={() => {
                setSelectedId(null);
                refresh();
              }}
              onChange={(g) => {
                setGames((prev) => prev.map((x) => (x.id === g.id ? g : x)));
              }}
              onLaunch={() => api.launchGame(selected.id).then(refresh)}
              onOpenSessions={() => setSessionsForId(selected.id)}
            />
          ) : null}
        </main>
      </div>

      {showSettings && (
        <SettingsPanel
          settings={settings}
          onChange={(s) => {
            setSettings(s);
            applyChrome(s);
          }}
          onClose={() => {
            setShowSettings(false);
            refresh();
          }}
        />
      )}

      {toast && <div className="toast">{toast}</div>}
      {ghost && (
        <div
          className="drag-ghost"
          style={{
            width: ghost.w,
            height: ghost.h,
            transform: `translate3d(${ghost.x - ghost.w / 2}px, ${ghost.y - 28}px, 0)`,
            ["--card-accent" as string]: ghost.accent || "var(--accent)",
          }}
        >
          {ghost.cover ? (
            <img src={ghost.cover} alt="" />
          ) : (
            <div className="drag-ghost-ph">{ghost.name.slice(0, 1).toUpperCase()}</div>
          )}
          <span>{ghost.name}</span>
        </div>
      )}
    </div>
  );
}
