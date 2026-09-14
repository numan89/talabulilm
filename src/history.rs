//! Local watch history: a small JSON file of recently played videos, most
//! recent first, so the History view can offer "continue watching" and
//! track which ones have actually been watched to the end.

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::ytdlp::{PlaybackOutcome, Video};

const MAX_ENTRIES: usize = 200;

/// Resume points within this many seconds of the very start, or of the
/// end, aren't worth seeking to — just start over.
const RESUME_EDGE_SECS: f64 = 5.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub title: String,
    pub channel: String,
    /// Unix timestamp (seconds) as a string — avoids pulling in a
    /// date/time-formatting crate just for a "recently watched" list.
    pub watched_at: String,
    #[serde(default)]
    pub position_secs: Option<f64>,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    #[serde(default)]
    pub finished: bool,
}

impl HistoryEntry {
    pub fn to_video(&self) -> Video {
        Video { id: self.id.clone(), title: self.title.clone(), channel: self.channel.clone(), duration: String::new() }
    }

    /// A short "Xm/h/d ago" label, computed from `watched_at` without
    /// needing a date/time crate.
    pub fn relative_time(&self) -> String {
        let then: u64 = self.watched_at.parse().unwrap_or(0);
        let now = now_unix();
        let diff = now.saturating_sub(then);
        if diff < 60 {
            "just now".to_string()
        } else if diff < 3600 {
            format!("{}m ago", diff / 60)
        } else if diff < 86400 {
            format!("{}h ago", diff / 3600)
        } else {
            format!("{}d ago", diff / 86400)
        }
    }

    /// Where Enter on this entry should resume from: `None` plays from
    /// the start (finished, never got a tracked position, or the
    /// tracked position is right at either edge already).
    pub fn resume_from(&self) -> Option<f64> {
        if self.finished {
            return None;
        }
        let pos = self.position_secs?;
        if pos < RESUME_EDGE_SECS {
            return None;
        }
        if let Some(dur) = self.duration_secs
            && dur - pos < RESUME_EDGE_SECS
        {
            return None;
        }
        Some(pos)
    }

    /// Status label for the History list: "Finished", "0:03 / 12:34 ·
    /// 0%" while partway through, or "opened, no progress tracked" when
    /// mpv's IPC never reported a position (too short a session for it
    /// to connect, or an mpv build without IPC support) — deliberately
    /// not worded like "watched", since this only means the entry exists,
    /// not that it was finished.
    pub fn progress_label(&self) -> String {
        if self.finished {
            return "Finished".to_string();
        }
        match (self.position_secs, self.duration_secs) {
            (Some(pos), Some(dur)) if dur > 0.0 => {
                let pct = (pos / dur * 100.0).round() as u32;
                format!("{} / {} \u{00b7} {pct}%", format_hms(pos), format_hms(dur))
            }
            (Some(pos), None) => format!("{} in, duration unknown", format_hms(pos)),
            _ => "opened, no progress tracked".to_string(),
        }
    }
}

fn format_hms(secs: f64) -> String {
    let total = secs.round().max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn load() -> Vec<HistoryEntry> {
    let content = match std::fs::read_to_string(paths::history_file()) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn save(entries: &[HistoryEntry]) {
    let path = paths::history_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string(entries) {
        let _ = std::fs::write(path, data);
    }
}

/// The ids of every video marked *finished* — used to keep search results
/// from re-surfacing something already watched to the end. A video merely
/// opened and quit out of early stays searchable (and is still reachable,
/// and resumable, from History itself) rather than disappearing after a
/// few seconds' peek.
pub fn finished_ids() -> HashSet<String> {
    load().into_iter().filter(|e| e.finished).map(|e| e.id).collect()
}

/// Records a video's playback outcome, most-recent-first, deduplicating
/// by id and capping the list so it doesn't grow forever.
pub fn record_watch(video: &Video, outcome: PlaybackOutcome) {
    let mut entries = load();
    entries.retain(|e| e.id != video.id);
    entries.insert(
        0,
        HistoryEntry {
            id: video.id.clone(),
            title: video.title.clone(),
            channel: video.channel.clone(),
            watched_at: now_unix().to_string(),
            position_secs: outcome.position_secs,
            duration_secs: outcome.duration_secs,
            finished: outcome.finished,
        },
    );
    entries.truncate(MAX_ENTRIES);
    save(&entries);
}

/// Forces an entry to "Finished" regardless of tracked position (e.g.
/// playback was watched to the end in a way mpv's IPC didn't catch).
pub fn mark_finished(id: &str) {
    let mut entries = load();
    if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
        e.finished = true;
    }
    save(&entries);
}

/// Removes one entry from history — it becomes searchable again.
pub fn remove(id: &str) {
    let mut entries = load();
    entries.retain(|e| e.id != id);
    save(&entries);
}
