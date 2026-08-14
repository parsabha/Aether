import { motion } from "motion/react";
import type { PointerEvent } from "react";
import type { Game } from "../../lib/types";
import { AutoPlayMedia } from "../media/AutoPlayMedia";

function fmtPlaytime(ms: number) {
  if (!ms || ms < 1000) return null;
  const sec = Math.floor(ms / 1000);
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m`;
  return `${sec}s`;
}

function fmtDate(ts?: number | null) {
  if (!ts) return null;
  const diff = Date.now() - ts;
  if (diff < 60_000) return "Just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  if (diff < 604_800_000) return `${Math.floor(diff / 86_400_000)}d ago`;
  try {
    return new Date(ts).toLocaleDateString();
  } catch {
    return null;
  }
}

function cardSub(game: Game) {
  if (game.missing) return "Game files not found";
  return fmtPlaytime(game.playtimeMs) || fmtDate(game.lastPlayed) || "Not played yet";
}

export function GameCard({
  game,
  onOpen,
  onLaunch,
  canDrag,
  onPointerDown,
  dragging,
  reduceMotion,
  suppressOpen,
}: {
  game: Game;
  onOpen: () => void;
  onLaunch: () => void;
  canDrag?: boolean;
  onPointerDown?: (e: PointerEvent) => void;
  dragging?: boolean;
  reduceMotion?: boolean;
  suppressOpen?: () => boolean;
}) {
  const accent = game.accent || "var(--accent)";

  return (
    <div
      className={[
        "game-card-wrap",
        canDrag ? "can-drag" : "",
        dragging ? "is-dragging" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      data-gid={game.id}
      onPointerDown={canDrag ? onPointerDown : undefined}
    >
      <motion.div
        role="button"
        tabIndex={0}
        className={`game-card ${game.missing ? "is-missing" : ""} ${canDrag ? "can-drag" : ""}`}
        style={{ ["--card-accent" as string]: accent }}
        onClick={() => {
          if (suppressOpen?.()) return;
          onOpen();
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onOpen();
          }
        }}
        whileTap={reduceMotion || dragging ? undefined : { scale: 0.98 }}
        transition={{ type: "spring", stiffness: 520, damping: 28 }}
      >
        <div className="game-card-art">
          {game.coverUrl ? (
            <AutoPlayMedia
              src={game.coverUrl}
              kind={game.coverKind}
              poster={game.coverPoster || undefined}
              className="game-card-media"
            />
          ) : game.iconUrl ? (
            <img src={game.iconUrl} alt="" className="game-card-icon" draggable={false} />
          ) : (
            <div className="game-card-ph">{game.name.slice(0, 1).toUpperCase()}</div>
          )}
        </div>

        <div className="game-card-overlay">
          <span className="game-card-name">{game.name}</span>
          <span className="game-card-sub">{cardSub(game)}</span>
        </div>

        {game.running && (
          <span className="card-running">
            <i className="run-dot" />
            Running
          </span>
        )}
        {game.favorite && <span className="fav-mark">★</span>}
        {game.missing && <span className="card-missing">Unavailable</span>}

        {!game.missing && (
          <span
            className="game-card-play"
            role="button"
            tabIndex={-1}
            onClick={(e) => {
              e.stopPropagation();
              onLaunch();
            }}
            onPointerDown={(e) => e.stopPropagation()}
          >
            <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden>
              <path d="M8 5.5v13l11-6.5z" fill="currentColor" />
            </svg>
          </span>
        )}
      </motion.div>
    </div>
  );
}
