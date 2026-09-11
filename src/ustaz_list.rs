//! Curated ustaz names shown by the UstazList view. Simpler than the bash
//! version's multi-location file search: the default list is embedded in
//! the binary at compile time (`include_str!`), so there's no separate
//! data file to install or lose track of. A user's own edits live in a
//! seeded copy under their data dir.

use std::path::PathBuf;

use crate::paths;

const DEFAULT_LIST: &str = include_str!("../ustaz_list.txt");

fn user_copy_path() -> PathBuf {
    paths::data_dir().join("ustaz_list.txt")
}

/// Seeds the user's editable copy on first run. Safe to call every
/// startup — it's a no-op once the file exists.
pub fn ensure_seeded() {
    let path = user_copy_path();
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, DEFAULT_LIST);
}

/// Loads names: `TALABULILM_USTAZ_LIST` env override if set, else the
/// user's (seeded) copy, else the embedded default as a last resort.
pub fn load() -> Vec<String> {
    let path = match std::env::var("TALABULILM_USTAZ_LIST") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => user_copy_path(),
    };
    let content = std::fs::read_to_string(&path).unwrap_or_else(|_| DEFAULT_LIST.to_string());
    parse(&content)
}

fn parse(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ignoring_comments_and_blank_lines() {
        let names = parse("# comment\n\nAzhar Idrus\n  Bakhiet  \n\n# another\n");
        assert_eq!(names, vec!["Azhar Idrus".to_string(), "Bakhiet".to_string()]);
    }

    #[test]
    fn embedded_default_parses_to_at_least_one_name() {
        assert!(!parse(DEFAULT_LIST).is_empty());
    }
}
