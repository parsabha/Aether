import { getCurrentWindow } from "@tauri-apps/api/window";

export function TitleBar({
  onToggleIsland,
  islandOn,
}: {
  onToggleIsland: () => void;
  islandOn: boolean;
}) {
  const win = getCurrentWindow();
  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <img className="brand-mark" src="/aether-icon.png" alt="" />
        <span className="brand-name">Aether</span>
      </div>
      <div className="titlebar-spacer" data-tauri-drag-region />
      <div className="titlebar-actions">
        <button
          className={`tb-btn ${islandOn ? "is-on" : ""}`}
          title="Toggle Dynamic Island"
          onClick={onToggleIsland}
        >
          <svg viewBox="0 0 24 24" width="16" height="16">
            <rect x="4" y="8" width="16" height="8" rx="4" fill="none" stroke="currentColor" strokeWidth="1.6" />
          </svg>
        </button>
        <button className="tb-btn" onClick={() => win.minimize()}>
          <svg viewBox="0 0 24 24" width="14" height="14"><path d="M5 12h14" stroke="currentColor" strokeWidth="1.8" /></svg>
        </button>
        <button
          className="tb-btn"
          onClick={async () => {
            if (await win.isMaximized()) await win.unmaximize();
            else await win.maximize();
          }}
        >
          <svg viewBox="0 0 24 24" width="13" height="13"><rect x="5" y="5" width="14" height="14" rx="2" fill="none" stroke="currentColor" strokeWidth="1.6" /></svg>
        </button>
        <button className="tb-btn tb-close" onClick={() => win.close()}>
          <svg viewBox="0 0 24 24" width="14" height="14"><path d="M6 6l12 12M18 6L6 18" stroke="currentColor" strokeWidth="1.8" /></svg>
        </button>
      </div>
    </header>
  );
}

export function Sidebar({
  filter,
  setFilter,
  categories,
  counts,
  onAdd,
  onSettings,
}: {
  filter: string;
  setFilter: (f: string) => void;
  categories: string[];
  counts: { all: number; favorite: number; running: number };
  onAdd: () => void;
  onSettings: () => void;
}) {
  return (
    <aside className="sidebar">
      <div className="sidebar-glass">
        <button type="button" className="add-btn" onClick={onAdd}>
          <span className="add-ico" aria-hidden>
            <svg viewBox="0 0 24 24" fill="none">
              <path d="M12 5v14M5 12h14" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" />
            </svg>
          </span>
          <span className="add-text">
            Add Games
            <small>Import .exe or shortcut</small>
          </span>
        </button>
        <nav className="nav">
          {(
            [
              ["all", "Library", counts.all],
              ["favorite", "Favorites", counts.favorite],
              ["recent", "Recent", null],
              ["running", "Playing", counts.running],
            ] as const
          ).map(([id, label, count]) => (
            <button
              key={id}
              className={`nav-item ${filter === id ? "active" : ""}`}
              onClick={() => setFilter(id)}
            >
              <span>{label}</span>
              {count != null && <em>{count}</em>}
            </button>
          ))}
        </nav>
        {categories.length > 0 && (
          <>
            <div className="nav-label">Categories</div>
            <nav className="nav">
              {categories.map((c) => (
                <button
                  key={c}
                  className={`nav-item ${filter === `cat:${c}` ? "active" : ""}`}
                  onClick={() => setFilter(`cat:${c}`)}
                >
                  <span>{c}</span>
                </button>
              ))}
            </nav>
          </>
        )}
        <button className="ghost-btn" onClick={onSettings}>
          <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden>
            <circle cx="12" cy="12" r="3" fill="none" stroke="currentColor" strokeWidth="1.7" />
            <path d="M12 3.5v2.2M12 18.3v2.2M4.9 6.5l1.6 1.6M17.5 16l1.6 1.6M3.5 12h2.2M18.3 12h2.2M4.9 17.5l1.6-1.6M17.5 8l1.6-1.6" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
          </svg>
          Settings
        </button>
      </div>
    </aside>
  );
}
