use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct Db {
    pub conn: Arc<Mutex<Connection>>,
    #[allow(dead_code)]
    pub root: PathBuf,
    pub media: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub name: String,
    pub exe_path: Option<String>,
    pub args: String,
    pub cwd: Option<String>,
    pub cover: Option<String>,
    pub cover_webm: Option<String>,
    pub cover_poster: Option<String>,
    pub banner: Option<String>,
    pub banner_webm: Option<String>,
    pub banner_poster: Option<String>,
    pub icon: Option<String>,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub accent: Option<String>,
    pub sort_order: i64,
    pub playtime_ms: i64,
    pub launch_count: i64,
    pub last_played: Option<i64>,
    pub added_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDto {
    #[serde(flatten)]
    pub game: Game,
    pub cover_url: Option<String>,
    pub cover_kind: String,
    pub banner_url: Option<String>,
    pub banner_kind: String,
    pub icon_url: Option<String>,
    pub screenshot_urls: Vec<ScreenshotDto>,
    pub running: bool,
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotDto {
    pub path: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub accent: String,
    pub sort: String,
    pub view: String,
    pub island_enabled: bool,
    pub island_on_top: bool,
    pub island_display: Option<u32>,
    pub blur: String,
    pub reduce_motion: bool,
    pub launch_on_startup: bool,
    pub start_minimized: bool,
    pub launch_as_admin: bool,
    pub screenshot_hotkey_enabled: bool,
    pub screenshot_hotkey: String,
    pub nebula_imported: bool,
    pub theme: String,
    /// When a library game is running, widen the island into a live metrics strip.
    pub overlay_enabled: bool,
    pub overlay_show_fps: bool,
    pub overlay_show_cpu_usage: bool,
    pub overlay_show_gpu_usage: bool,
    pub overlay_show_cpu_temp: bool,
    pub overlay_show_vram: bool,
    /// Bearer token from steamgriddb.com/profile/preferences
    pub steamgriddb_api_key: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            accent: "#5ac8ff".into(),
            sort: "custom".into(),
            view: "grid".into(),
            island_enabled: true,
            island_on_top: true,
            island_display: None,
            blur: "acrylic".into(),
            reduce_motion: false,
            launch_on_startup: true,
            start_minimized: false,
            launch_as_admin: true,
            screenshot_hotkey_enabled: true,
            screenshot_hotkey: "F9".into(),
            nebula_imported: false,
            theme: "aether".into(),
            overlay_enabled: false,
            overlay_show_fps: true,
            overlay_show_cpu_usage: true,
            overlay_show_gpu_usage: true,
            overlay_show_cpu_temp: true,
            overlay_show_vram: true,
            steamgriddb_api_key: String::new(),
        }
    }
}

impl Db {
    pub fn open(app_data: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(app_data).map_err(|e| e.to_string())?;
        let media = app_data.join("media");
        std::fs::create_dir_all(&media).map_err(|e| e.to_string())?;
        let db_path = app_data.join("aether.db");
        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;",
        )
        .map_err(|e| e.to_string())?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            root: app_data.to_path_buf(),
            media,
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS meta (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS games (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              exe_path TEXT,
              args TEXT NOT NULL DEFAULT '',
              cwd TEXT,
              cover TEXT,
              cover_webm TEXT,
              cover_poster TEXT,
              banner TEXT,
              banner_webm TEXT,
              banner_poster TEXT,
              icon TEXT,
              description TEXT NOT NULL DEFAULT '',
              category TEXT NOT NULL DEFAULT 'Games',
              tags_json TEXT NOT NULL DEFAULT '[]',
              favorite INTEGER NOT NULL DEFAULT 0,
              accent TEXT,
              sort_order INTEGER NOT NULL DEFAULT 0,
              playtime_ms INTEGER NOT NULL DEFAULT 0,
              launch_count INTEGER NOT NULL DEFAULT 0,
              last_played INTEGER,
              added_at INTEGER NOT NULL,
              deleted_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS screenshots (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              game_id TEXT NOT NULL,
              path TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              FOREIGN KEY(game_id) REFERENCES games(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS settings (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS media_jobs (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              game_id TEXT NOT NULL,
              slot TEXT NOT NULL,
              src TEXT NOT NULL,
              status TEXT NOT NULL DEFAULT 'pending',
              created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_games_name ON games(name);
            CREATE INDEX IF NOT EXISTS idx_games_last_played ON games(last_played);
            CREATE INDEX IF NOT EXISTS idx_screenshots_game ON screenshots(game_id);
            CREATE TABLE IF NOT EXISTS play_sessions (
              id TEXT PRIMARY KEY,
              game_id TEXT NOT NULL,
              started_at INTEGER NOT NULL,
              ended_at INTEGER,
              duration_ms INTEGER NOT NULL DEFAULT 0,
              avg_fps REAL,
              min_fps REAL,
              max_fps REAL,
              avg_cpu REAL,
              avg_gpu REAL,
              avg_cpu_temp REAL,
              sample_count INTEGER NOT NULL DEFAULT 0,
              fps_1_low REAL,
              avg_vram REAL,
              avg_gpu_temp REAL,
              avg_ram REAL,
              avg_gpu_power REAL,
              FOREIGN KEY(game_id) REFERENCES games(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS session_samples (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              session_id TEXT NOT NULL,
              t_ms INTEGER NOT NULL,
              fps REAL,
              cpu_usage REAL,
              gpu_usage REAL,
              cpu_temp_c REAL,
              vram_used_mb REAL,
              vram_total_mb REAL,
              gpu_temp_c REAL,
              ram_used_mb REAL,
              ram_total_mb REAL,
              frame_time_ms REAL,
              gpu_power_w REAL,
              FOREIGN KEY(session_id) REFERENCES play_sessions(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_game ON play_sessions(game_id, started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_samples_session ON session_samples(session_id, t_ms);
            "#,
        )
        .map_err(|e| e.to_string())?;

        // Additive columns for richer session telemetry (safe on existing DBs).
        for sql in [
            "ALTER TABLE session_samples ADD COLUMN gpu_temp_c REAL",
            "ALTER TABLE session_samples ADD COLUMN ram_used_mb REAL",
            "ALTER TABLE session_samples ADD COLUMN ram_total_mb REAL",
            "ALTER TABLE session_samples ADD COLUMN frame_time_ms REAL",
            "ALTER TABLE session_samples ADD COLUMN gpu_power_w REAL",
            "ALTER TABLE play_sessions ADD COLUMN fps_1_low REAL",
            "ALTER TABLE play_sessions ADD COLUMN avg_vram REAL",
            "ALTER TABLE play_sessions ADD COLUMN avg_gpu_temp REAL",
            "ALTER TABLE play_sessions ADD COLUMN avg_ram REAL",
            "ALTER TABLE play_sessions ADD COLUMN avg_gpu_power REAL",
        ] {
            let _ = conn.execute(sql, []);
        }

        let defaults = serde_json::to_string(&Settings::default()).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (id, json) VALUES (1, ?1)",
            params![defaults],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn game_media_dir(&self, id: &str) -> PathBuf {
        let dir = self.media.join(id);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    pub fn get_settings(&self) -> Result<Settings, String> {
        let conn = self.conn.lock();
        let json: String = conn
            .query_row("SELECT json FROM settings WHERE id = 1", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }

    pub fn set_settings(&self, patch: serde_json::Value) -> Result<Settings, String> {
        let mut current = self.get_settings()?;
        let cur = serde_json::to_value(&current).map_err(|e| e.to_string())?;
        let mut map = cur.as_object().cloned().unwrap_or_default();
        if let Some(obj) = patch.as_object() {
            for (k, v) in obj {
                map.insert(k.clone(), v.clone());
            }
        }
        current = serde_json::from_value(serde_json::Value::Object(map)).map_err(|e| e.to_string())?;
        let json = serde_json::to_string(&current).map_err(|e| e.to_string())?;
        let conn = self.conn.lock();
        conn.execute("UPDATE settings SET json = ?1 WHERE id = 1", params![json])
            .map_err(|e| e.to_string())?;
        Ok(current)
    }

    fn row_to_game(row: &rusqlite::Row<'_>) -> rusqlite::Result<Game> {
        let tags_json: String = row.get("tags_json")?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        Ok(Game {
            id: row.get("id")?,
            name: row.get("name")?,
            exe_path: row.get("exe_path")?,
            args: row.get("args")?,
            cwd: row.get("cwd")?,
            cover: row.get("cover")?,
            cover_webm: row.get("cover_webm")?,
            cover_poster: row.get("cover_poster")?,
            banner: row.get("banner")?,
            banner_webm: row.get("banner_webm")?,
            banner_poster: row.get("banner_poster")?,
            icon: row.get("icon")?,
            description: row.get("description")?,
            category: row.get("category")?,
            tags,
            favorite: row.get::<_, i64>("favorite")? != 0,
            accent: row.get("accent")?,
            sort_order: row.get("sort_order")?,
            playtime_ms: row.get("playtime_ms")?,
            launch_count: row.get("launch_count")?,
            last_played: row.get("last_played")?,
            added_at: row.get("added_at")?,
            deleted_at: row.get("deleted_at")?,
        })
    }

    pub fn list_games(&self) -> Result<Vec<Game>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT * FROM games WHERE deleted_at IS NULL ORDER BY sort_order ASC, name ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], Self::row_to_game)
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_game(&self, id: &str) -> Result<Option<Game>, String> {
        let conn = self.conn.lock();
        conn.query_row("SELECT * FROM games WHERE id = ?1", params![id], Self::row_to_game)
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn upsert_game(&self, g: &Game) -> Result<(), String> {
        let tags = serde_json::to_string(&g.tags).map_err(|e| e.to_string())?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO games (
              id, name, exe_path, args, cwd, cover, cover_webm, cover_poster,
              banner, banner_webm, banner_poster, icon, description, category,
              tags_json, favorite, accent, sort_order, playtime_ms, launch_count,
              last_played, added_at, deleted_at
            ) VALUES (
              ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23
            )
            ON CONFLICT(id) DO UPDATE SET
              name=excluded.name, exe_path=excluded.exe_path, args=excluded.args, cwd=excluded.cwd,
              cover=excluded.cover, cover_webm=excluded.cover_webm, cover_poster=excluded.cover_poster,
              banner=excluded.banner, banner_webm=excluded.banner_webm, banner_poster=excluded.banner_poster,
              icon=excluded.icon, description=excluded.description, category=excluded.category,
              tags_json=excluded.tags_json, favorite=excluded.favorite, accent=excluded.accent,
              sort_order=excluded.sort_order, playtime_ms=excluded.playtime_ms,
              launch_count=excluded.launch_count, last_played=excluded.last_played,
              added_at=excluded.added_at, deleted_at=excluded.deleted_at
            "#,
            params![
                g.id,
                g.name,
                g.exe_path,
                g.args,
                g.cwd,
                g.cover,
                g.cover_webm,
                g.cover_poster,
                g.banner,
                g.banner_webm,
                g.banner_poster,
                g.icon,
                g.description,
                g.category,
                tags,
                if g.favorite { 1 } else { 0 },
                g.accent,
                g.sort_order,
                g.playtime_ms,
                g.launch_count,
                g.last_played,
                g.added_at,
                g.deleted_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn remove_game(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM games WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn reorder_games(&self, ordered_ids: &[String]) -> Result<(), String> {
        let conn = self.conn.lock();
        for (i, id) in ordered_ids.iter().enumerate() {
            conn.execute(
                "UPDATE games SET sort_order = ?1 WHERE id = ?2",
                params![(i as i64) * 10, id],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn list_screenshots(&self, game_id: &str) -> Result<Vec<String>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT path FROM screenshots WHERE game_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![game_id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn add_screenshot(&self, game_id: &str, path: &str) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO screenshots (game_id, path, created_at) VALUES (?1, ?2, ?3)",
            params![game_id, path, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn remove_screenshot(&self, game_id: &str, path: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM screenshots WHERE game_id = ?1 AND path = ?2",
            params![game_id, path],
        )
        .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    pub fn enqueue_media_job(&self, game_id: &str, slot: &str, src: &str) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO media_jobs (game_id, slot, src, status, created_at) VALUES (?1,?2,?3,'pending',?4)",
            params![game_id, slot, src, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn take_pending_jobs(&self, limit: usize) -> Result<Vec<(i64, String, String, String)>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, game_id, slot, src FROM media_jobs WHERE status = 'pending' ORDER BY id ASC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        for (id, _, _, _) in &out {
            conn.execute(
                "UPDATE media_jobs SET status = 'running' WHERE id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(out)
    }

    pub fn finish_job(&self, id: i64, ok: bool) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE media_jobs SET status = ?1 WHERE id = ?2",
            params![if ok { "done" } else { "error" }, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_play_session(&self, id: &str, game_id: &str, started_at: i64) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO play_sessions (id, game_id, started_at) VALUES (?1, ?2, ?3)",
            params![id, game_id, started_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_session_samples(&self, session_id: &str, samples: &[SessionSample]) -> Result<(), String> {
        if samples.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock();
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO session_samples (
                       session_id, t_ms, fps, cpu_usage, gpu_usage, cpu_temp_c,
                       vram_used_mb, vram_total_mb, gpu_temp_c, ram_used_mb, ram_total_mb,
                       frame_time_ms, gpu_power_w
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                )
                .map_err(|e| e.to_string())?;
            for s in samples {
                stmt.execute(params![
                    session_id,
                    s.t_ms,
                    s.fps,
                    s.cpu_usage,
                    s.gpu_usage,
                    s.cpu_temp_c,
                    s.vram_used_mb,
                    s.vram_total_mb,
                    s.gpu_temp_c,
                    s.ram_used_mb,
                    s.ram_total_mb,
                    s.frame_time_ms,
                    s.gpu_power_w,
                ])
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn finalize_play_session(&self, session_id: &str, ended_at: i64) -> Result<(), String> {
        let conn = self.conn.lock();
        let started_at: i64 = conn
            .query_row(
                "SELECT started_at FROM play_sessions WHERE id = ?1",
                params![session_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let duration_ms = (ended_at - started_at).max(0);

        let fps_vals: Vec<f64> = {
            let mut stmt = conn
                .prepare("SELECT fps FROM session_samples WHERE session_id = ?1 AND fps IS NOT NULL ORDER BY fps ASC")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![session_id], |r| r.get::<_, f64>(0))
                .map_err(|e| e.to_string())?;
            let mut v = Vec::new();
            for r in rows {
                if let Ok(x) = r {
                    v.push(x);
                }
            }
            v
        };
        let fps_1_low = if fps_vals.is_empty() {
            None
        } else {
            let idx = ((fps_vals.len() as f64) * 0.01).floor() as usize;
            Some(fps_vals[idx.min(fps_vals.len() - 1)])
        };

        let (avg_fps, min_fps, max_fps, avg_cpu, avg_gpu, avg_temp, avg_vram, avg_gpu_temp, avg_ram, avg_power, count): (
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            i64,
        ) = conn
            .query_row(
                "SELECT
                   AVG(fps), MIN(fps), MAX(fps),
                   AVG(cpu_usage), AVG(gpu_usage), AVG(cpu_temp_c),
                   AVG(vram_used_mb), AVG(gpu_temp_c), AVG(ram_used_mb), AVG(gpu_power_w),
                   COUNT(*)
                 FROM session_samples WHERE session_id = ?1",
                params![session_id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                        r.get(10)?,
                    ))
                },
            )
            .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE play_sessions SET
               ended_at = ?1,
               duration_ms = ?2,
               avg_fps = ?3,
               min_fps = ?4,
               max_fps = ?5,
               avg_cpu = ?6,
               avg_gpu = ?7,
               avg_cpu_temp = ?8,
               sample_count = ?9,
               fps_1_low = ?10,
               avg_vram = ?11,
               avg_gpu_temp = ?12,
               avg_ram = ?13,
               avg_gpu_power = ?14
             WHERE id = ?15",
            params![
                ended_at,
                duration_ms,
                avg_fps,
                min_fps,
                max_fps,
                avg_cpu,
                avg_gpu,
                avg_temp,
                count,
                fps_1_low,
                avg_vram,
                avg_gpu_temp,
                avg_ram,
                avg_power,
                session_id,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_play_sessions(&self, game_id: &str) -> Result<Vec<PlaySessionSummary>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, game_id, started_at, ended_at, duration_ms,
                        avg_fps, min_fps, max_fps, avg_cpu, avg_gpu, avg_cpu_temp, sample_count,
                        fps_1_low, avg_vram, avg_gpu_temp, avg_ram, avg_gpu_power
                 FROM play_sessions
                 WHERE game_id = ?1
                 ORDER BY started_at DESC
                 LIMIT 100",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![game_id], |r| {
                Ok(PlaySessionSummary {
                    id: r.get(0)?,
                    game_id: r.get(1)?,
                    started_at: r.get(2)?,
                    ended_at: r.get(3)?,
                    duration_ms: r.get(4)?,
                    avg_fps: r.get(5)?,
                    min_fps: r.get(6)?,
                    max_fps: r.get(7)?,
                    avg_cpu: r.get(8)?,
                    avg_gpu: r.get(9)?,
                    avg_cpu_temp: r.get(10)?,
                    sample_count: r.get(11)?,
                    fps_1_low: r.get(12)?,
                    avg_vram: r.get(13)?,
                    avg_gpu_temp: r.get(14)?,
                    avg_ram: r.get(15)?,
                    avg_gpu_power: r.get(16)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn get_play_session(&self, session_id: &str) -> Result<Option<PlaySessionDetail>, String> {
        let summary = {
            let conn = self.conn.lock();
            conn.query_row(
                "SELECT id, game_id, started_at, ended_at, duration_ms,
                        avg_fps, min_fps, max_fps, avg_cpu, avg_gpu, avg_cpu_temp, sample_count,
                        fps_1_low, avg_vram, avg_gpu_temp, avg_ram, avg_gpu_power
                 FROM play_sessions WHERE id = ?1",
                params![session_id],
                |r| {
                    Ok(PlaySessionSummary {
                        id: r.get(0)?,
                        game_id: r.get(1)?,
                        started_at: r.get(2)?,
                        ended_at: r.get(3)?,
                        duration_ms: r.get(4)?,
                        avg_fps: r.get(5)?,
                        min_fps: r.get(6)?,
                        max_fps: r.get(7)?,
                        avg_cpu: r.get(8)?,
                        avg_gpu: r.get(9)?,
                        avg_cpu_temp: r.get(10)?,
                        sample_count: r.get(11)?,
                        fps_1_low: r.get(12)?,
                        avg_vram: r.get(13)?,
                        avg_gpu_temp: r.get(14)?,
                        avg_ram: r.get(15)?,
                        avg_gpu_power: r.get(16)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())?
        };
        let Some(summary) = summary else {
            return Ok(None);
        };
        let samples = {
            let conn = self.conn.lock();
            let mut stmt = conn
                .prepare(
                    "SELECT t_ms, fps, cpu_usage, gpu_usage, cpu_temp_c, vram_used_mb, vram_total_mb,
                            gpu_temp_c, ram_used_mb, ram_total_mb, frame_time_ms, gpu_power_w
                     FROM session_samples WHERE session_id = ?1 ORDER BY t_ms ASC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![session_id], |r| {
                    Ok(SessionSample {
                        t_ms: r.get(0)?,
                        fps: r.get(1)?,
                        cpu_usage: r.get(2)?,
                        gpu_usage: r.get(3)?,
                        cpu_temp_c: r.get(4)?,
                        vram_used_mb: r.get(5)?,
                        vram_total_mb: r.get(6)?,
                        gpu_temp_c: r.get(7)?,
                        ram_used_mb: r.get(8)?,
                        ram_total_mb: r.get(9)?,
                        frame_time_ms: r.get(10)?,
                        gpu_power_w: r.get(11)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r.map_err(|e| e.to_string())?);
            }
            out
        };
        Ok(Some(PlaySessionDetail { summary, samples }))
    }

    pub fn delete_play_session(&self, session_id: &str) -> Result<bool, String> {
        let conn = self.conn.lock();
        let n = conn
            .execute("DELETE FROM play_sessions WHERE id = ?1", params![session_id])
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSample {
    pub t_ms: i64,
    pub fps: Option<f32>,
    pub cpu_usage: Option<f32>,
    pub gpu_usage: Option<f32>,
    pub cpu_temp_c: Option<f32>,
    pub vram_used_mb: Option<f32>,
    pub vram_total_mb: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub ram_used_mb: Option<f32>,
    pub ram_total_mb: Option<f32>,
    pub frame_time_ms: Option<f32>,
    pub gpu_power_w: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySessionSummary {
    pub id: String,
    pub game_id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_ms: i64,
    pub avg_fps: Option<f64>,
    pub min_fps: Option<f64>,
    pub max_fps: Option<f64>,
    pub avg_cpu: Option<f64>,
    pub avg_gpu: Option<f64>,
    pub avg_cpu_temp: Option<f64>,
    pub sample_count: i64,
    pub fps_1_low: Option<f64>,
    pub avg_vram: Option<f64>,
    pub avg_gpu_temp: Option<f64>,
    pub avg_ram: Option<f64>,
    pub avg_gpu_power: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaySessionDetail {
    pub summary: PlaySessionSummary,
    pub samples: Vec<SessionSample>,
}
