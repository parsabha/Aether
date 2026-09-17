//! Per-session performance sampling (FPS / CPU / GPU / temp / VRAM / RAM / power).
//!
//! Samples are written while a library game is tracked in `AppState.running`.
//! The overlay sampler pushes metrics; game start/end owns session lifecycle.
//! Extra fields are for the Results page only — not shown on the island HUD.

use crate::db::{Db, PlaySessionDetail, PlaySessionSummary, SessionSample};
use parking_lot::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

struct ActiveSession {
    session_id: String,
    #[allow(dead_code)]
    game_id: String,
    started_at: i64,
    last_flush: Instant,
    pending: Vec<SessionSample>,
    last_sample_wall: i64,
}

static ACTIVE: Mutex<Option<ActiveSession>> = Mutex::new(None);
/// 1.5 Hz while playing — charts stay useful, disk/CPU stay quiet.
const SAMPLE_INTERVAL_MS: i64 = 1500;
const FLUSH_EVERY: Duration = Duration::from_millis(6000);
const FLUSH_BATCH: usize = 12;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn begin_session(db: &Db, game_id: &str) -> Result<(String, i64), String> {
    let id = Uuid::new_v4().to_string();
    let started = now_ms();
    db.insert_play_session(&id, game_id, started)?;
    {
        let mut guard = ACTIVE.lock();
        if let Some(prev) = guard.take() {
            let _ = flush_active(db, prev);
        }
        *guard = Some(ActiveSession {
            session_id: id.clone(),
            game_id: game_id.to_string(),
            started_at: started,
            last_flush: Instant::now(),
            pending: Vec::new(),
            last_sample_wall: 0,
        });
    }
    Ok((id, started))
}

pub fn resume_session(session_id: &str, game_id: &str, started_at: i64) {
    let mut guard = ACTIVE.lock();
    if guard
        .as_ref()
        .map(|a| a.session_id == session_id)
        .unwrap_or(false)
    {
        return;
    }
    *guard = Some(ActiveSession {
        session_id: session_id.to_string(),
        game_id: game_id.to_string(),
        started_at,
        last_flush: Instant::now(),
        pending: Vec::new(),
        last_sample_wall: 0,
    });
}

#[allow(dead_code)]
pub fn active_session_id() -> Option<String> {
    ACTIVE.lock().as_ref().map(|a| a.session_id.clone())
}

pub fn record_sample(db: &Db, sample: SessionSample) {
    let wall = now_ms();
    let mut guard = ACTIVE.lock();
    let Some(active) = guard.as_mut() else {
        return;
    };
    if wall - active.last_sample_wall < SAMPLE_INTERVAL_MS {
        return;
    }
    active.last_sample_wall = wall;
    let mut sample = sample;
    sample.t_ms = (wall - active.started_at).max(0);
    active.pending.push(sample);
    let should_flush =
        active.pending.len() >= FLUSH_BATCH || active.last_flush.elapsed() >= FLUSH_EVERY;
    if should_flush {
        let session_id = active.session_id.clone();
        let batch = std::mem::take(&mut active.pending);
        active.last_flush = Instant::now();
        drop(guard);
        let _ = db.insert_session_samples(&session_id, &batch);
    }
}

pub fn end_session(db: &Db, session_id: &str, ended_at: i64) -> Result<(), String> {
    let taken = {
        let mut guard = ACTIVE.lock();
        if guard
            .as_ref()
            .map(|a| a.session_id == session_id)
            .unwrap_or(false)
        {
            guard.take()
        } else {
            None
        }
    };
    if let Some(active) = taken {
        let _ = flush_active(db, active);
    }
    db.finalize_play_session(session_id, ended_at)
}

fn flush_active(db: &Db, mut active: ActiveSession) -> Result<(), String> {
    if !active.pending.is_empty() {
        db.insert_session_samples(&active.session_id, &active.pending)?;
        active.pending.clear();
    }
    Ok(())
}

pub fn list_sessions(db: &Db, game_id: &str) -> Result<Vec<PlaySessionSummary>, String> {
    db.list_play_sessions(game_id)
}

pub fn get_session(db: &Db, session_id: &str) -> Result<Option<PlaySessionDetail>, String> {
    db.get_play_session(session_id)
}

pub fn delete_session(db: &Db, session_id: &str) -> Result<bool, String> {
    {
        let mut guard = ACTIVE.lock();
        if guard
            .as_ref()
            .map(|a| a.session_id == session_id)
            .unwrap_or(false)
        {
            *guard = None;
        }
    }
    db.delete_play_session(session_id)
}
