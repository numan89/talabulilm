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
