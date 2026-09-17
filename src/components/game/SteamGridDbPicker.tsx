import { openUrl } from "@tauri-apps/plugin-opener";
import { motion } from "motion/react";
import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { Game, SgdbAsset, SgdbGame } from "../../lib/types";

type Slot = "cover" | "banner" | "icon";

function slotTitle(slot: Slot) {
  switch (slot) {
    case "cover":
      return "Cover (Grid)";
    case "banner":
      return "Banner (Hero)";
    case "icon":
      return "Icon";
  }
}

export function ArtSourceMenu({
  slot,
  open,
  onClose,
  onSteamGridDb,
  onUpload,
}: {
  slot: Slot;
  open: boolean;
  onClose: () => void;
  onSteamGridDb: () => void;
  onUpload: () => void;
}) {
  if (!open) return null;
  return (
    <div className="modal-backdrop art-source-backdrop" onClick={onClose}>
      <motion.div
        className="art-source-menu"
        onClick={(e) => e.stopPropagation()}
        initial={{ opacity: 0, y: 10, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        transition={{ duration: 0.18 }}
      >
        <header>
          <h3>Add {slotTitle(slot)}</h3>
          <p>SteamGridDB or a file from your PC</p>
        </header>
        <button type="button" className="art-source-option primary" onClick={onSteamGridDb}>
          <span className="art-source-mark" aria-hidden>
            SGDB
          </span>
          <span>
            <strong>SteamGridDB</strong>
            <small>Browse grids, heroes, and icons — click to apply</small>
          </span>
        </button>
        <button type="button" className="art-source-option" onClick={onUpload}>
          <span className="art-source-mark faint" aria-hidden>
            PC
          </span>
          <span>
            <strong>Upload from PC</strong>
            <small>Choose a local image or video</small>
          </span>
        </button>
        <button type="button" className="btn art-source-cancel" onClick={onClose}>
          Cancel
        </button>
      </motion.div>
    </div>
  );
}

export function SteamGridDbPicker({
  game,
  slot,
  onClose,
  onApplied,
}: {
  game: Game;
  slot: Slot;
  onClose: () => void;
  onApplied: (g: Game) => void;
}) {
  const [query, setQuery] = useState(game.name);
  const [games, setGames] = useState<SgdbGame[]>([]);
  const [selected, setSelected] = useState<SgdbGame | null>(null);
  const [assets, setAssets] = useState<SgdbAsset[]>([]);
  const [loading, setLoading] = useState(false);
  const [applying, setApplying] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        if (selected) {
          setSelected(null);
          setAssets([]);
        } else {
          onClose();
        }
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose, selected]);

  useEffect(() => {
    const t = setTimeout(() => {
      void runSearch(query);
    }, 280);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const runSearch = async (term: string) => {
    const q = term.trim();
    if (!q) {
      setGames([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const results = await api.sgdbSearch(q);
      setGames(results);
      setSelected(null);
      setAssets([]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setGames([]);
    } finally {
      setLoading(false);
    }
  };

  const pickGame = async (g: SgdbGame) => {
    setSelected(g);
    setLoading(true);
    setError(null);
    try {
      const list = await api.sgdbListAssets(g.id, slot);
      setAssets(list);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setAssets([]);
    } finally {
      setLoading(false);
    }
  };

  const apply = async (asset: SgdbAsset) => {
    setApplying(asset.id);
    setError(null);
    try {
      const updated = await api.sgdbApplyAsset(game.id, slot, asset.url, asset.mime);
      onApplied(updated);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setApplying(null);
    }
  };

  return (
    <div className="modal-backdrop sgdb-backdrop" onClick={onClose}>
      <motion.div
        className="sgdb-panel"
        onClick={(e) => e.stopPropagation()}
        initial={{ opacity: 0, y: 16, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        transition={{ duration: 0.2 }}
      >
        <div className="sgdb-head">
          <div>
            <p className="sgdb-kicker">SteamGridDB × Aether</p>
            <h2>{slotTitle(slot)}</h2>
            <p className="settings-sub">
              Search, then click an image to download and place it for {game.name}
            </p>
          </div>
          <div className="sgdb-head-actions">
            <button
              type="button"
              className="btn"
              onClick={() => openUrl("https://www.steamgriddb.com/").catch(() => {})}
            >
              Open website
            </button>
            <button type="button" className="tb-btn" onClick={onClose} aria-label="Close">
              ✕
            </button>
          </div>
        </div>

        <form
          className="sgdb-search"
          onSubmit={(e) => {
            e.preventDefault();
            void runSearch(query);
          }}
        >
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search SteamGridDB…"
            autoFocus
          />
          <button type="submit" className="btn primary" disabled={loading}>
            {loading ? "…" : "Search"}
          </button>
        </form>

        {error && <p className="sgdb-error">{error}</p>}

        <div className="sgdb-body">
          {!selected ? (
            <div className="sgdb-games">
              {games.length === 0 && !loading ? (
                <p className="sgdb-empty">Search for a game to browse artwork.</p>
              ) : (
                games.map((g) => (
                  <button key={g.id} type="button" className="sgdb-game" onClick={() => void pickGame(g)}>
                    <strong>{g.name}</strong>
                    {g.verified ? <span className="sgdb-badge">Verified</span> : null}
                  </button>
                ))
              )}
            </div>
          ) : (
            <div className="sgdb-assets-wrap">
              <button type="button" className="sgdb-back" onClick={() => { setSelected(null); setAssets([]); }}>
                ← {selected.name}
              </button>
              {assets.length === 0 && !loading ? (
                <p className="sgdb-empty">No {slotTitle(slot).toLowerCase()} found for this title.</p>
              ) : (
                <div className={`sgdb-assets sgdb-assets-${slot}`}>
                  {assets.map((a) => (
                    <button
                      key={a.id}
                      type="button"
                      className="sgdb-asset"
                      disabled={applying !== null}
                      onClick={() => void apply(a)}
                      title={`${a.width || "?"}×${a.height || "?"} · click to apply`}
                    >
                      <img src={a.thumb || a.url} alt="" loading="lazy" />
                      {applying === a.id && <span className="sgdb-asset-busy">Applying…</span>}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </motion.div>
    </div>
  );
}
