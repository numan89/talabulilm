//! XDG base directory resolution, matching the bash version's
//! `${XDG_DATA_HOME:-$HOME/.local/share}/talabulilm` conventions.

use std::path::PathBuf;

fn home_dir() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    match std::env::var("XDG_DATA_HOME") {
        Ok(v) if !v.is_empty() => PathBuf::from(v).join("talabulilm"),
        _ => home_dir().join(".local/share/talabulilm"),
    }
}

pub fn state_dir() -> PathBuf {
    match std::env::var("XDG_STATE_HOME") {
        Ok(v) if !v.is_empty() => PathBuf::from(v).join("talabulilm"),
        _ => home_dir().join(".local/state/talabulilm"),
    }
}

pub fn history_file() -> PathBuf {
    state_dir().join("history.json")
}

/// Shared with gem-say.sh's SUPER+P "ilmuakhirat" row and the waybar
/// ilmuakhirat-*.sh scripts: `-t`'s background audio playback (see
/// `ytdlp::play_audio_background`) writes to these same files so the
/// existing waybar module can show/control that session too, and so only
/// one such session is ever "now playing" regardless of which side
/// started it.
pub fn ilmuakhirat_sock() -> PathBuf {
    home_dir().join(".cache/gem-say/ilmuakhirat.sock")
}

pub fn ilmuakhirat_playing_file() -> PathBuf {
    home_dir().join(".cache/gem-say/ilmuakhirat_playing.json")
}
