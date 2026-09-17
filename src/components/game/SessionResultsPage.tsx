import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { api } from "../../lib/api";
import type { Game, PlaySessionDetail, PlaySessionSummary, SessionSample } from "../../lib/types";

type MetricKey =
  | "fps"
  | "frameTimeMs"
  | "gpuUsage"
  | "cpuUsage"
  | "cpuTempC"
  | "gpuTempC"
  | "vramUsedMb"
  | "ramUsedMb"
  | "gpuPowerW";

const CHARTS: {
  key: MetricKey;
  label: string;
  color: string;
  unit: string;
  digits: number;
}[] = [
  { key: "fps", label: "FPS", color: "#5ac8ff", unit: "", digits: 0 },
  { key: "frameTimeMs", label: "Frame time", color: "#22d3ee", unit: " ms", digits: 1 },
  { key: "gpuUsage", label: "GPU usage", color: "#3dd68c", unit: "%", digits: 0 },
  { key: "cpuUsage", label: "CPU usage", color: "#f5c542", unit: "%", digits: 0 },
  { key: "cpuTempC", label: "CPU temp", color: "#ff6b4a", unit: "°C", digits: 0 },
  { key: "gpuTempC", label: "GPU temp", color: "#f97316", unit: "°C", digits: 0 },
  { key: "vramUsedMb", label: "VRAM", color: "#a78bfa", unit: " MB", digits: 0 },
  { key: "ramUsedMb", label: "System RAM", color: "#e879f9", unit: " MB", digits: 0 },
  { key: "gpuPowerW", label: "GPU power", color: "#fb7185", unit: " W", digits: 0 },
];

function fmtDur(ms: number) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
  return `${m}:${String(sec).padStart(2, "0")}`;
}

function fmtNum(n?: number | null, digits = 0) {
  if (n == null || !Number.isFinite(n)) return "—";
  return n.toFixed(digits);
}

function sampleVal(s: SessionSample, key: MetricKey): number | null {
  const v = s[key];
  return v == null || !Number.isFinite(v) ? null : v;
}

function nearestSample(samples: SessionSample[], tMs: number): SessionSample | null {
  if (!samples.length) return null;
  let lo = 0;
  let hi = samples.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (samples[mid].tMs < tMs) lo = mid + 1;
    else hi = mid;
  }
  const a = samples[Math.max(0, lo - 1)];
  const b = samples[lo];
  if (!a) return b;
  if (!b) return a;
  return Math.abs(a.tMs - tMs) <= Math.abs(b.tMs - tMs) ? a : b;
}

function MetricChart({
  samples,
  metric,
  maxT,
  cursorT,
  onCursor,
}: {
  samples: SessionSample[];
  metric: (typeof CHARTS)[number];
  maxT: number;
  cursorT: number;
  onCursor: (t: number) => void;
}) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(700);
  const h = 160;
  const pad = { l: 48, r: 16, t: 16, b: 28 };
  const iw = Math.max(40, w - pad.l - pad.r);
  const ih = h - pad.t - pad.b;

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setW(Math.max(320, el.getBoundingClientRect().width));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const { minY, maxY, path, area } = useMemo(() => {
    let min = Infinity;
    let max = -Infinity;
    for (const s of samples) {
      const v = sampleVal(s, metric.key);
      if (v == null) continue;
      min = Math.min(min, v);
      max = Math.max(max, v);
    }
    if (!Number.isFinite(min)) {
      return { minY: 0, maxY: 1, path: "", area: "" };
    }
    if (min === max) {
      min = Math.max(0, min - 1);
      max = max + 1;
    } else {
      const padY = (max - min) * 0.1;
      min = Math.max(0, min - padY);
      max += padY;
    }
    const pts: string[] = [];
    let started = false;
    let firstX = 0;
    let lastX = 0;
    for (const s of samples) {
      const v = sampleVal(s, metric.key);
      if (v == null) {
        started = false;
        continue;
      }
      const x = maxT > 0 ? (s.tMs / maxT) * iw : 0;
      const y = ih - ((v - min) / Math.max(0.001, max - min)) * ih;
      if (!started) firstX = x;
      pts.push(`${started ? "L" : "M"}${x.toFixed(1)} ${y.toFixed(1)}`);
      lastX = x;
      started = true;
    }
    const line = pts.join(" ");
    const areaPath = line
      ? `${line} L${lastX.toFixed(1)} ${ih.toFixed(1)} L${firstX.toFixed(1)} ${ih.toFixed(1)} Z`
      : "";
    return { minY: min, maxY: max, path: line, area: areaPath };
  }, [samples, metric.key, maxT, iw, ih]);

  const onMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const el = wrapRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const x = e.clientX - rect.left - pad.l;
    onCursor(Math.max(0, Math.min(maxT, (x / iw) * maxT)));
  };

  const at = nearestSample(samples, cursorT);
  const atVal = at ? sampleVal(at, metric.key) : null;
  const cursorX = pad.l + (maxT > 0 ? (cursorT / maxT) * iw : 0);
  const hasData = samples.some((s) => sampleVal(s, metric.key) != null);

  if (!hasData) return null;

  return (
    <div className="sr-chart-card">
      <div className="sr-chart-head">
        <div className="sr-chart-title">
          <span className="sess-swatch" style={{ background: metric.color }} />
          <strong>{metric.label}</strong>
        </div>
        <div className="sr-chart-live" style={{ color: metric.color }}>
          {fmtNum(atVal, metric.digits)}
          {metric.unit}
          <span className="sr-chart-live-t">{fmtDur(cursorT)}</span>
        </div>
      </div>
      <div className="sr-chart" ref={wrapRef} onPointerMove={onMove} onPointerDown={onMove}>
        <svg width={w} height={h}>
          <defs>
            <linearGradient id={`sr-g-${metric.key}`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={metric.color} stopOpacity="0.35" />
              <stop offset="100%" stopColor={metric.color} stopOpacity="0.02" />
            </linearGradient>
          </defs>
          {[0, 0.5, 1].map((p) => (
            <line
              key={p}
              x1={pad.l}
              x2={pad.l + iw}
              y1={pad.t + ih * p}
              y2={pad.t + ih * p}
              className="sess-grid"
            />
          ))}
          <g transform={`translate(${pad.l},${pad.t})`}>
            {area && <path d={area} fill={`url(#sr-g-${metric.key})`} />}
            {path && (
              <path
                d={path}
                fill="none"
                stroke={metric.color}
                strokeWidth="2.25"
                strokeLinejoin="round"
                strokeLinecap="round"
              />
            )}
          </g>
          <line x1={cursorX} x2={cursorX} y1={pad.t} y2={pad.t + ih} className="sess-cursor" />
          <text x={8} y={pad.t + 10} className="sess-axis-label">
            {fmtNum(maxY, metric.digits)}
          </text>
          <text x={8} y={pad.t + ih} className="sess-axis-label">
            {fmtNum(minY, metric.digits)}
          </text>
          <text x={pad.l} y={h - 6} className="sess-axis-label">
            0:00
          </text>
          <text x={pad.l + iw / 2} y={h - 6} textAnchor="middle" className="sess-axis-label">
            {fmtDur(maxT / 2)}
          </text>
          <text x={pad.l + iw} y={h - 6} textAnchor="end" className="sess-axis-label">
            {fmtDur(maxT)}
          </text>
        </svg>
      </div>
    </div>
  );
}

export function SessionResultsPage({
  game,
  onBack,
}: {
  game: Game;
  onBack: () => void;
}) {
  const [sessions, setSessions] = useState<PlaySessionSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [compareId, setCompareId] = useState<string | null>(null);
  const [detail, setDetail] = useState<PlaySessionDetail | null>(null);
  const [compare, setCompare] = useState<PlaySessionDetail | null>(null);
  const [cursorT, setCursorT] = useState(0);

  const reload = useCallback(async () => {
    const list = await api.listPlaySessions(game.id);
    setSessions(list);
    setSelectedId((prev) => {
      if (prev && list.some((s) => s.id === prev)) return prev;
      return list.find((s) => s.sampleCount > 0)?.id ?? list[0]?.id ?? null;
    });
  }, [game.id]);

  useEffect(() => {
    reload();
    let un: (() => void) | undefined;
    api.onSessionsChanged((id) => {
      if (id === game.id) reload();
    }).then((u) => {
      un = u;
    });
    return () => un?.();
  }, [game.id, reload]);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    api.getPlaySession(selectedId).then((d) => {
      if (!cancelled) {
        setDetail(d);
        setCursorT(0);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [selectedId]);

  useEffect(() => {
    if (!compareId) {
      setCompare(null);
      return;
    }
    let cancelled = false;
    api.getPlaySession(compareId).then((d) => {
      if (!cancelled) setCompare(d);
    });
    return () => {
      cancelled = true;
    };
  }, [compareId]);

  const selected = sessions.find((s) => s.id === selectedId) || null;
  const samples = detail?.samples || [];
  const maxT = Math.max(
    selected?.durationMs || 1,
    samples[samples.length - 1]?.tMs || 1,
  );
  const at = nearestSample(samples, cursorT);

  return (
    <div className="sr-page">
      <header className="sr-top">
        <button type="button" className="sr-back" onClick={onBack}>
          ← Back
        </button>
        <div className="sr-top-title">
          <h1>{game.name}</h1>
          <span>Session results</span>
        </div>
      </header>

      <div className="sr-body">
        <aside className="sr-list">
          {sessions.length === 0 ? (
            <div className="sess-empty compact">
              <p>No sessions yet</p>
            </div>
          ) : (
            sessions.map((s) => (
              <div
                key={s.id}
                role="button"
                tabIndex={0}
                className={`sess-item ${s.id === selectedId ? "active" : ""} ${s.id === compareId ? "compare" : ""}`}
                onClick={() => setSelectedId(s.id)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setSelectedId(s.id);
                  }
                }}
              >
                <div className="sess-item-top">
                  <strong>{new Date(s.startedAt).toLocaleString()}</strong>
                  <span>{fmtDur(s.durationMs)}</span>
                </div>
                <div className="sess-item-stats">
                  <span>avg {fmtNum(s.avgFps, 0)} FPS</span>
                  <span>1% {fmtNum(s.fps1Low, 0)}</span>
                  <span>GPU {fmtNum(s.avgGpu, 0)}%</span>
                </div>
                <div className="sess-item-actions">
                  <button
                    type="button"
                    className="sess-mini"
                    onClick={(e) => {
                      e.stopPropagation();
                      setCompareId((c) => (c === s.id ? null : s.id));
                    }}
                  >
                    {compareId === s.id ? "Clear" : "Compare"}
                  </button>
                  <button
                    type="button"
                    className="sess-mini danger"
                    onClick={async (e) => {
                      e.stopPropagation();
                      if (!confirm("Delete this session?")) return;
                      await api.deletePlaySession(s.id);
                      if (compareId === s.id) setCompareId(null);
                      await reload();
                    }}
                  >
                    Delete
                  </button>
                </div>
              </div>
            ))
          )}
        </aside>

        <main className="sr-main">
          {!selected || !detail ? (
            <div className="sess-empty">
              <p>Select a session</p>
            </div>
          ) : detail.samples.length === 0 ? (
            <div className="sess-empty">
              <p>No samples in this session</p>
              <span>Keep Aether elevated while playing so metrics can be captured.</span>
            </div>
          ) : (
            <>
              <div className="sr-summary">
                <div>
                  <label>Duration</label>
                  <strong>{fmtDur(selected.durationMs)}</strong>
                </div>
                <div>
                  <label>Avg FPS</label>
                  <strong>{fmtNum(selected.avgFps, 1)}</strong>
                </div>
                <div>
                  <label>1% low</label>
                  <strong>{fmtNum(selected.fps1Low, 0)}</strong>
                </div>
                <div>
                  <label>Min / Max</label>
                  <strong>
                    {fmtNum(selected.minFps, 0)} / {fmtNum(selected.maxFps, 0)}
                  </strong>
                </div>
                <div>
                  <label>Avg GPU</label>
                  <strong>{fmtNum(selected.avgGpu, 0)}%</strong>
                </div>
                <div>
                  <label>Avg CPU</label>
                  <strong>{fmtNum(selected.avgCpu, 0)}%</strong>
                </div>
                <div>
                  <label>CPU temp</label>
                  <strong>{fmtNum(selected.avgCpuTemp, 0)}°C</strong>
                </div>
                <div>
                  <label>GPU temp</label>
                  <strong>{fmtNum(selected.avgGpuTemp, 0)}°C</strong>
                </div>
                <div>
                  <label>Avg VRAM</label>
                  <strong>{fmtNum(selected.avgVram, 0)} MB</strong>
                </div>
                <div>
                  <label>Avg RAM</label>
                  <strong>{fmtNum(selected.avgRam, 0)} MB</strong>
                </div>
                <div>
                  <label>GPU power</label>
                  <strong>{fmtNum(selected.avgGpuPower, 0)} W</strong>
                </div>
                <div>
                  <label>Samples</label>
                  <strong>{selected.sampleCount}</strong>
                </div>
              </div>

              <div className="sr-inspect">
                <div className="sr-inspect-time">{fmtDur(cursorT)}</div>
                <div className="sr-inspect-grid">
                  {CHARTS.map((m) => {
                    const v = at ? sampleVal(at, m.key) : null;
                    if (v == null && !samples.some((s) => sampleVal(s, m.key) != null)) return null;
                    const cv =
                      compare && compareId !== selectedId
                        ? sampleVal(nearestSample(compare.samples, cursorT) || ({} as SessionSample), m.key)
                        : null;
                    return (
                      <div key={m.key} className="sr-inspect-item">
                        <span className="sess-swatch" style={{ background: m.color }} />
                        <span>{m.label}</span>
                        <strong style={{ color: m.color }}>
                          {fmtNum(v, m.digits)}
                          {m.unit}
                        </strong>
                        {cv != null && (
                          <em>
                            vs {fmtNum(cv, m.digits)}
                            {m.unit}
                          </em>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>

              <input
                className="sess-scrub"
                type="range"
                min={0}
                max={maxT}
                step={250}
                value={cursorT}
                onChange={(e) => setCursorT(Number(e.target.value))}
              />

              <div className="sr-charts">
                {CHARTS.map((m) => (
                  <MetricChart
                    key={m.key}
                    samples={samples}
                    metric={m}
                    maxT={maxT}
                    cursorT={cursorT}
                    onCursor={setCursorT}
                  />
                ))}
                {compare && compareId !== selectedId && (
                  <p className="sess-hint">Compare session loaded — values show as “vs …” in the readout.</p>
                )}
              </div>
            </>
          )}
        </main>
      </div>
    </div>
  );
}
