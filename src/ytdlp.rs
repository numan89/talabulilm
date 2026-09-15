//! All `yt-dlp`/`mpv` subprocess orchestration. talabulilm never talks to
//! YouTube directly — `yt-dlp` already solves a moving-target problem
//! (signature ciphers, PO tokens, SABR-only streaming) that would be a
//! maintenance trap to duplicate, so we shell out to it exactly like the
//! bash version did.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub title: String,
}

/// Percent-encodes a string for a URL query, UTF-8 safe (mirrors the bash
/// version's `urlencode`, but trivial in Rust since we can iterate bytes
/// directly instead of shelling out to `od`).
fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'~' | b'_' | b'-' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Runs `yt-dlp` with `--flat-playlist --print <template>` and returns its
/// stdout split into non-empty lines. Centralizes the flags every listing
/// call shares.
async fn run_flat_playlist(target: &str, template: &str, limit: u32) -> Result<Vec<String>> {
    let output = Command::new("yt-dlp")
        .arg(target)
        .args(["--flat-playlist", "--no-warnings", "--playlist-end"])
        .arg(limit.to_string())
        .args(["--print", template])
        .output()
        .await
        .context("failed to run yt-dlp (is it installed?)")?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

fn parse_video_line(line: &str) -> Option<Video> {
    let mut fields = line.splitn(4, '\t');
    Some(Video {
        id: fields.next()?.to_string(),
        title: fields.next()?.to_string(),
        channel: fields.next()?.to_string(),
        duration: fields.next()?.to_string(),
    })
}

/// Flat keyword search across all of YouTube (no channel/playlist
/// grouping) — the fallback path when no channel matches, or the fast
/// path before grouping is wired up.
pub async fn search_videos(query: &str, limit: u32) -> Result<Vec<Video>> {
    let target = format!("ytsearch{limit}:Ceramah Ustaz {query}");
    let lines = run_flat_playlist(
        &target,
        "%(id)s\t%(title)s\t%(uploader)s\t%(duration_string)s",
        limit,
    )
    .await?;
    Ok(lines.iter().filter_map(|l| parse_video_line(l)).collect())
}

/// Finds YouTube channels matching the query, best-followed first (same
/// channel-filtered search URL and subscriber-count ranking the bash
/// version used).
pub async fn search_channels(query: &str, limit: u32) -> Result<Vec<Channel>> {
    let q = urlencode(&format!("Ceramah Ustaz {query}"));
    let target = format!("https://www.youtube.com/results?search_query={q}&sp=EgIQAg%253D%253D");
    let lines = run_flat_playlist(&target, "%(id)s\t%(title)s\t%(channel_follower_count)s", limit * 3).await?;

    let mut channels: Vec<(Channel, i64)> = lines
        .iter()
        .filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            let id = f.next()?.to_string();
            let title = f.next()?.to_string();
            let followers: i64 = f.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            Some((Channel { id, title }, followers))
        })
        .collect();

    channels.sort_by(|a, b| b.1.cmp(&a.1));
    channels.truncate(limit as usize);
    Ok(channels.into_iter().map(|(c, _)| c).collect())
}

/// A channel's public uploads feed (`/videos` tab). The uploader field is
/// unreliable there (`yt-dlp` often reports "NA"), so we override it with
/// the channel title we already know, same as the bash version's `awk`
/// post-processing.
pub async fn channel_uploads(channel_id: &str, channel_title: &str, limit: u32) -> Result<Vec<Video>> {
    let target = format!("https://www.youtube.com/channel/{channel_id}/videos");
    let lines = run_flat_playlist(&target, "%(id)s\t%(title)s\t%(duration_string)s", limit).await?;
    Ok(lines
        .iter()
        .filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            let id = f.next()?.to_string();
            let title = f.next()?.to_string();
            let duration = f.next()?.to_string();
            Some(Video { id, title, channel: channel_title.to_string(), duration })
        })
        .collect())
}

/// A channel's playlists (`/playlists` tab). Channels with no playlists
/// tab make `yt-dlp` exit nonzero with empty stdout; since we don't check
/// the exit status here, that already degrades gracefully to "zero
/// playlists" rather than an error.
pub async fn channel_playlists(channel_id: &str, limit: u32) -> Result<Vec<(String, String)>> {
    let target = format!("https://www.youtube.com/channel/{channel_id}/playlists");
    let lines = run_flat_playlist(&target, "%(id)s\t%(title)s", limit).await?;
    Ok(lines
        .iter()
        .filter_map(|l| {
            let mut f = l.splitn(2, '\t');
            Some((f.next()?.to_string(), f.next()?.to_string()))
        })
        .collect())
}

/// A single playlist's videos.
pub async fn playlist_videos(playlist_id: &str, limit: u32) -> Result<Vec<Video>> {
    let target = format!("https://www.youtube.com/playlist?list={playlist_id}");
    let lines = run_flat_playlist(
        &target,
        "%(id)s\t%(title)s\t%(uploader)s\t%(duration_string)s",
        limit,
    )
    .await?;
    Ok(lines.iter().filter_map(|l| parse_video_line(l)).collect())
}

/// Quality knobs, mirroring the bash version's `format_string`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Best,
    P1080,
    P720,
    P480,
    P360,
    Worst,
    AudioOnly,
}

impl Quality {
    pub fn label(self) -> &'static str {
        match self {
            Quality::Best => "best",
            Quality::P1080 => "1080p",
            Quality::P720 => "720p",
            Quality::P480 => "480p",
            Quality::P360 => "360p",
            Quality::Worst => "worst",
            Quality::AudioOnly => "audio",
        }
    }

    fn format_string(self) -> String {
        match self {
            Quality::Best => "bestvideo*+bestaudio/best".to_string(),
            Quality::Worst => "worstvideo*+worstaudio/worst".to_string(),
            Quality::AudioOnly => "bestaudio/best".to_string(),
            Quality::P1080 => "bestvideo*[height<=1080]+bestaudio/best[height<=1080]".to_string(),
            Quality::P720 => "bestvideo*[height<=720]+bestaudio/best[height<=720]".to_string(),
            Quality::P480 => "bestvideo*[height<=480]+bestaudio/best[height<=480]".to_string(),
            Quality::P360 => "bestvideo*[height<=360]+bestaudio/best[height<=360]".to_string(),
        }
    }

    /// Caps to at most 480p for `--terminal-video`: a terminal-graphics
    /// box is far smaller than a real window, so fetching `Best`/1080p
    /// buys no visible detail there, only data. Leaves an already-lower
    /// explicit choice (480p, 360p, Worst, audio) alone.
    fn capped_for_terminal_video(self) -> Quality {
        match self {
            Quality::Best | Quality::P1080 | Quality::P720 => Quality::P480,
            other => other,
        }
    }

    pub fn cycle_next(self) -> Self {
        match self {
            Quality::Best => Quality::P1080,
            Quality::P1080 => Quality::P720,
            Quality::P720 => Quality::P480,
            Quality::P480 => Quality::P360,
            Quality::P360 => Quality::Worst,
            Quality::Worst => Quality::AudioOnly,
            Quality::AudioOnly => Quality::Best,
        }
    }
}

/// Where mpv playback ended up, gathered over its JSON IPC socket while it
/// ran — how far in it got and whether it reached the end on its own —
/// so the caller can record real watch progress instead of just "played
/// this at some point".
#[derive(Debug, Clone, Copy, Default)]
pub struct PlaybackOutcome {
    pub position_secs: Option<f64>,
    pub duration_secs: Option<f64>,
    /// True on a clean end-of-file, or (as a fallback, in case mpv is
    /// killed right at the end before the eof event reaches us) once
    /// position is within 5% of duration.
    pub finished: bool,
}

/// Plays a video URL in `mpv`, blocking until the player exits. Callers on
/// the TUI side are expected to leave the alternate screen first (mpv
/// opens its own window via `--force-window=yes`, but sharing a terminal
/// with a raw-mode TUI underneath it is asking for trouble).
///
/// `resume_from` seeks to that position on start (used for "continue
/// watching" from History); mpv is given an IPC socket so we can observe
/// `time-pos`/`duration` as it plays and know where playback actually
/// left off, regardless of how the user closed the player.
///
/// `terminal_video` swaps the separate mpv window for mpv's own `tct`
/// video output, which draws the frame as truecolor blocks straight into
/// the terminal we just gave mpv back — opt-in only (started via
/// `--terminal-video`) since it's much lower fidelity than a real window.
/// Sized small and pinned to the top-left (mpv always starts drawing at
/// the current cursor position, the terminal's top-left right after its
/// alternate-screen switch) rather than stretched to fill whatever
/// terminal it happens to run in. No info overlay: every way tried to
/// show one (mpv's OSD — not composited at all by `tct`; `drawtext` baked
/// into the frame — illegible at a small box's resolution; Kitty
/// graphics with the screen kept — worked, but only on Kitty-protocol
/// terminals and added real fragility for a "little info" ask) cost more
/// than it was worth. Keeping this the plain, boring version.
pub async fn play(url: &str, title: &str, quality: Quality, resume_from: Option<f64>, terminal_video: bool) -> Result<PlaybackOutcome> {
    let socket_path = std::env::temp_dir().join(format!("talabulilm-mpv-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket_path);

    // A small terminal box is far smaller than a real window, so
    // fetching Best/1080p buys no visible detail there, only data —
    // capped down unless the caller already picked something lower.
    let effective_quality = if terminal_video { quality.capped_for_terminal_video() } else { quality };

    let mut cmd = Command::new("mpv");
    if terminal_video {
        let (cols, rows) = terminal_video_box_size();
        cmd.arg("--vo=tct")
            // mpv's normal console status line ("AV: 00:00:05 / 00:03:33
            // (2%)") writes straight to the terminal too, fighting
            // `tct`'s own frame-drawing escape codes over the same
            // cursor, causing visible flicker. `--quiet` turns it off;
            // playback progress still comes from the IPC socket below.
            .arg("--quiet")
            .arg(format!("--vo-tct-width={cols}"))
            .arg(format!("--vo-tct-height={rows}"));
    } else {
        cmd.arg("--force-window=yes");
    }
    cmd.arg(format!("--ytdl-format={}", effective_quality.format_string()))
        .arg(format!("--title={title}"))
        .arg(format!("--input-ipc-server={}", socket_path.display()));
    if let Some(start) = resume_from {
        cmd.arg(format!("--start={start}"));
    }
    cmd.arg(url);

    let mut child = cmd.spawn().context("failed to run mpv (is it installed?)")?;

    let state = Arc::new(Mutex::new(PlaybackOutcome::default()));
    let tracker = tokio::spawn(track_playback(socket_path.clone(), state.clone()));

    let status = child.wait().await.context("failed to wait on mpv")?;
    tracker.abort();
    let _ = std::fs::remove_file(&socket_path);

    if !status.success() {
        bail!("mpv exited with {status}");
    }

    let mut outcome = *state.lock().await;
    if let (Some(pos), Some(dur)) = (outcome.position_secs, outcome.duration_secs)
        && dur > 0.0
        && pos / dur >= 0.95
    {
        outcome.finished = true;
    }
    Ok(outcome)
}

/// Size of the `tct` box for `--terminal-video`: a fixed small size,
/// clamped to the real terminal so it still fits in a small window.
fn terminal_video_box_size() -> (u16, u16) {
    const DEFAULT_COLS: u16 = 64;
    const DEFAULT_ROWS: u16 = 20;
    const MIN_COLS: u16 = 20;
    const MIN_ROWS: u16 = 8;
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let cols = DEFAULT_COLS.min(term_cols).max(MIN_COLS.min(term_cols));
    let rows = DEFAULT_ROWS.min(term_rows).max(MIN_ROWS.min(term_rows));
    (cols, rows)
}

/// Connects to mpv's IPC socket (retrying briefly since mpv needs a
/// moment to create it after spawn) and observes `time-pos`/`duration`
/// plus the `end-file` event, updating `state` as they arrive. Degrades
/// silently — playback still works with no progress tracked — if the
/// socket never shows up (e.g. an mpv build without IPC support).
async fn track_playback(socket_path: PathBuf, state: Arc<Mutex<PlaybackOutcome>>) {
    let mut stream = None;
    for _ in 0..50 {
        match UnixStream::connect(&socket_path).await {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    let Some(stream) = stream else { return };
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

    let _ = write_half.write_all(b"{\"command\":[\"observe_property\",1,\"time-pos\"]}\n").await;
    let _ = write_half.write_all(b"{\"command\":[\"observe_property\",2,\"duration\"]}\n").await;

    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        match msg.get("event").and_then(Value::as_str) {
            Some("property-change") => {
                let name = msg.get("name").and_then(Value::as_str);
                let data = msg.get("data").and_then(Value::as_f64);
                let mut guard = state.lock().await;
                match name {
                    // Only overwrite with a real value: mpv sends one
                    // final property-change with no "data" (null) right
                    // as playback stops (quit/eof), and blindly applying
                    // that would wipe out the position we'd already
                    // tracked during actual playback.
                    Some("time-pos") if data.is_some() => guard.position_secs = data,
                    Some("duration") if data.is_some() => guard.duration_secs = data,
                    _ => {}
                }
            }
            Some("end-file") => {
                if msg.get("reason").and_then(Value::as_str) == Some("eof") {
                    state.lock().await.finished = true;
                }
            }
            _ => {}
        }
    }
}

/// Downloads a video URL via `yt-dlp` into `dir`, blocking until done.
pub async fn download(url: &str, dir: &str, quality: Quality) -> Result<()> {
    std::fs::create_dir_all(dir).ok();
    let status = Command::new("yt-dlp")
        .args(["-f", &quality.format_string()])
        .args(["-o", &format!("{dir}/%(uploader)s - %(title)s.%(ext)s")])
        .arg(url)
        .status()
        .await
        .context("failed to run yt-dlp (is it installed?)")?;

    if !status.success() {
        bail!("yt-dlp exited with {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_line() {
        let v = parse_video_line("abc123\tSome Title\tSome Channel\t42:10").unwrap();
        assert_eq!(v.id, "abc123");
        assert_eq!(v.title, "Some Title");
        assert_eq!(v.channel, "Some Channel");
        assert_eq!(v.duration, "42:10");
    }

    #[test]
    fn rejects_a_short_line() {
        assert!(parse_video_line("abc123\tonly two fields").is_none());
    }

    #[test]
    fn quality_cycle_is_a_full_loop() {
        let mut q = Quality::Best;
        for _ in 0..7 {
            q = q.cycle_next();
        }
        assert_eq!(q, Quality::Best);
    }
}
