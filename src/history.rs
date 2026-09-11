//! Local watch history: a small JSON file of recently played videos, most
//! recent first, so the History view can offer "continue watching".

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::ytdlp::Video;

const MAX_ENTRIES: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub title: String,
    pub channel: String,
    /// Unix timestamp (seconds) as a string — avoids pulling in a
    /// date/time-formatting crate just for a "recently watched" list.
    pub watched_at: String,
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

/// Records a watched video, most-recent-first, deduplicating by id and
/// capping the list so it doesn't grow forever.
pub fn record(video: &Video) {
    let mut entries = load();
    entries.retain(|e| e.id != video.id);
    entries.insert(
        0,
        HistoryEntry { id: video.id.clone(), title: video.title.clone(), channel: video.channel.clone(), watched_at: now_unix().to_string() },
    );
    entries.truncate(MAX_ENTRIES);

    let path = paths::history_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string(&entries) {
        let _ = std::fs::write(path, data);
    }
}
