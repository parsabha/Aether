import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { PlaySessionSummary } from "../../lib/types";

function fmtDur(ms: number) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m`;
  return `${s}s`;
}

function fmtNum(n?: number | null, digits = 0) {
  if (n == null || !Number.isFinite(n)) return "—";
  return n.toFixed(digits);
}

/** Compact teaser on the game page — 3 stats + link to full results. */
export function SessionTeaser({
  gameId,
  onOpenResults,
}: {
  gameId: string;
  onOpenResults: () => void;
}) {
  const [latest, setLatest] = useState<PlaySessionSummary | null>(null);
  const [count, setCount] = useState(0);

  const reload = useCallback(async () => {
    try {
      const list = await api.listPlaySessions(gameId);
      setCount(list.length);
      setLatest(list.find((s) => s.sampleCount > 0) || list[0] || null);
    } catch {
      setLatest(null);
      setCount(0);
    }
  }, [gameId]);

  useEffect(() => {
    reload();
    let un: (() => void) | undefined;
    api.onSessionsChanged((id) => {
      if (id === gameId) reload();
    }).then((u) => {
      un = u;
    });
    return () => un?.();
  }, [gameId, reload]);

  return (
    <section className="detail-section detail-sessions-teaser">
      <header className="detail-section-head">
        <h2>Sessions</h2>
        {count > 0 && <span className="detail-section-count">{count}</span>}
      </header>

      {!latest ? (
        <div className="sess-teaser-empty">
          <p>No sessions recorded yet</p>
          <span>Play while Aether is running to capture performance.</span>
        </div>
      ) : (
        <div className="sess-teaser">
          <div className="sess-teaser-stats">
            <div>
              <label>Last avg FPS</label>
              <strong>{fmtNum(latest.avgFps, 0)}</strong>
            </div>
            <div>
              <label>Avg GPU</label>
              <strong>{fmtNum(latest.avgGpu, 0)}%</strong>
            </div>
            <div>
              <label>Duration</label>
              <strong>{fmtDur(latest.durationMs)}</strong>
            </div>
          </div>
          <button type="button" className="sess-teaser-btn" onClick={onOpenResults}>
            Full results
          </button>
        </div>
      )}
    </section>
  );
}
