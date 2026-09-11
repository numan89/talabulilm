//! Disk cache for a query's group listing. Same semantics as the bash
//! version: keyed by normalized query text, 24h TTL, and versioned by app
//! version so a format change auto-invalidates old caches instead of
//! deserializing something stale into a shape that no longer matches.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use crate::groups::GroupRow;

pub const TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Turns a query into a filesystem-safe directory name: lowercased,
/// whitespace-collapsed, non-alphanumeric characters replaced. Using the
/// (sanitized) text itself rather than a hash keeps cache entries
/// inspectable on disk, and sidesteps needing a hash that's stable across
/// process runs.
fn sanitize_key(query: &str) -> String {
    let normalized = query.trim().to_lowercase();
    let words: Vec<&str> = normalized.split_whitespace().collect();
    let joined = words.join("_");
    let mut key: String = joined
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    key.truncate(100);
    if key.is_empty() { "_".to_string() } else { key }
}

pub fn path_for(data_dir: &Path, version: &str, query: &str) -> PathBuf {
    data_dir.join("cache").join(version).join(sanitize_key(query)).join("groups.json")
}

/// Loads a cached listing if the file exists and is younger than `TTL`.
pub fn load(path: &Path) -> Option<Vec<GroupRow>> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    if modified.elapsed().ok()? > TTL {
        return None;
    }
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save(path: &Path, groups: &[GroupRow]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string(groups)?;
    std::fs::write(path, data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_and_whitespace_to_the_same_key() {
        assert_eq!(sanitize_key("bakhiet"), sanitize_key("  Bakhiet  "));
        assert_eq!(sanitize_key("Ustaz Bakhiet"), sanitize_key("ustaz   bakhiet"));
    }

    #[test]
    fn sanitizes_filesystem_unsafe_characters() {
        let key = sanitize_key("sabar/dalam ujian?");
        assert!(key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    }
}
