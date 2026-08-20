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
            "#,
        )
        .map_err(|e| e.to_string())?;

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
}
