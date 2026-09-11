//! Channel → (playlists + a loose "OTHER" bucket) grouping. Mirrors the
//! bash version's `build_groups`: for one channel, fetch its uploads feed
//! and its playlists, fetch each playlist's own video list (bounded
//! concurrency), then any upload not found in any playlist becomes the
//! "OTHER" group.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::ytdlp::{self, Channel, Video};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GroupKind {
    Playlist { title: String },
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRow {
    pub channel: String,
    pub kind: GroupKind,
    pub videos: Vec<Video>,
}

/// Builds every group (playlists + OTHER) for one channel. Playlist video
/// listings are fetched concurrently, bounded by `parallel`, so a channel
/// with many playlists doesn't open dozens of `yt-dlp` processes at once.
pub async fn build_channel_groups(
    channel: Channel,
    uploads_limit: u32,
    playlist_limit: u32,
    parallel: usize,
) -> Result<Vec<GroupRow>> {
    let uploads = ytdlp::channel_uploads(&channel.id, &channel.title, uploads_limit).await?;
    let playlists = ytdlp::channel_playlists(&channel.id, playlist_limit).await.unwrap_or_default();

    let semaphore = Arc::new(Semaphore::new(parallel.max(1)));
    let mut handles = Vec::new();
    for (playlist_id, playlist_title) in playlists {
        let semaphore = semaphore.clone();
        handles.push(tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok();
            let videos = ytdlp::playlist_videos(&playlist_id, uploads_limit).await.unwrap_or_default();
            (playlist_title, videos)
        }));
    }

    let mut groups = Vec::new();
    let mut playlisted_ids: HashSet<String> = HashSet::new();
    for handle in handles {
        if let Ok((title, videos)) = handle.await {
            if videos.is_empty() {
                continue;
            }
            playlisted_ids.extend(videos.iter().map(|v| v.id.clone()));
            groups.push(GroupRow { channel: channel.title.clone(), kind: GroupKind::Playlist { title }, videos });
        }
    }

    let loose: Vec<Video> = uploads.into_iter().filter(|v| !playlisted_ids.contains(&v.id)).collect();
    if !loose.is_empty() {
        groups.push(GroupRow { channel: channel.title, kind: GroupKind::Other, videos: loose });
    }

    Ok(groups)
}
