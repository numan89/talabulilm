# talabulilm

A full-screen terminal app for watching **Ceramah Ustaz** (Islamic
lectures) — same idea as [ani-cli](https://github.com/pystardust/ani-cli),
but instead of anime it browses, groups, and streams ceramah videos, with
its own persistent UI and keybindings (like
[`cekhalal`](https://github.com/numan89/cekhalal), my other terminal
tool — same look, same feel).

Type a name or topic and it browses in two steps, like a mini video
library:

1. **Groups** — talabulilm finds matching YouTube channels, and for each
   one lists its `PL` playlists plus an `OTHER` bucket for uploads the
   channel never put in a playlist. Groups stream in live as each channel
   finishes loading, and a preview pane shows what's inside before you
   commit — no separate "open" step.
2. **Videos** — from inside the chosen channel/playlist. `Esc` goes back
   to the group list.

Then it plays straight into `mpv` via `yt-dlp` — no browser, no ads, no
downloads needed (unless you want one).

## Dependencies

- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp)
- [`mpv`](https://mpv.io/)

Install on Arch:

```sh
sudo pacman -S yt-dlp mpv
```

## Install

**Arch Linux (AUR):** `yay -S talabulilm` or `paru -S talabulilm` (or
manually: `git clone https://aur.archlinux.org/talabulilm.git && cd talabulilm && makepkg -si`)

**From source:**

```sh
git clone https://github.com/numan89/talabulilm.git
cd talabulilm
cargo build --release
./target/release/talabulilm
```

## Usage

```sh
talabulilm            # launch with an empty search box
talabulilm bakhiet    # launch and immediately search
```

### Keybindings

| Keys | Where | Does |
|---|---|---|
| Type, `Enter` | Search | Run a search |
| `Ctrl+U` | anywhere | Browse the curated ustaz list |
| `Ctrl+H` | anywhere | Watch history |
| `Tab` | anywhere | Jump between Search and the current results/groups pane |
| `↑`/`↓` (or `j`/`k`) | any list | Move selection |
| `Enter` (or `l`) | Groups | Open the highlighted group |
| `Enter` (or `l`) | Videos/History | Play the highlighted video |
| `d` | Videos | Download instead of playing |
| `[` / `]` | Videos | Cycle quality: best, 1080p, 720p, 480p, 360p, worst, audio-only |
| `Esc` (or `h`) | any list | Go back |
| `q` / `Ctrl+C` | anywhere | Quit |

If no channel matches your query, it falls back to a plain flat video
search and drops you straight into the Videos list.

## Customizing the ustaz list

The first time you run talabulilm, it copies its built-in default list to
`${XDG_DATA_HOME:-$HOME/.local/share}/talabulilm/ustaz_list.txt` — edit
that copy (one name per line; `#` comments and blank lines are ignored).
`Ctrl+U` fuzzy-filters these names as you type; picking one runs the same
browse flow as a free-text search, pre-filled with that name.

## History

Watched (or downloaded) videos are logged to
`${XDG_STATE_HOME:-$HOME/.local/state}/talabulilm/history.json` so
`Ctrl+H` can bring them back up, most-recent-first.

## Caching

Building a query's channel/playlist group listing is the slow part
(several `yt-dlp` calls). Results are cached per query under
`${XDG_DATA_HOME:-$HOME/.local/share}/talabulilm/cache/<version>/` for 24
hours, so repeating the same search is instant. The cache path is
versioned, so upgrading talabulilm never serves you a stale cache in a
format the new version doesn't expect.

## How it works

talabulilm is a Rust TUI (`ratatui` + `crossterm` + `tokio`) — no server,
no database. It never talks to YouTube directly: `yt-dlp` already solves
a moving-target problem (signature ciphers, PO tokens, SABR-only
streaming) that would be a maintenance trap to reimplement, so talabulilm
just orchestrates it as a subprocess, same as `mpv` for playback.

1. `ytdlp::search_channels` asks YouTube (filtered to channel results)
   for channels matching your query, ranked by subscriber count.
2. `groups::build_channel_groups` fetches each channel's playlists and
   its uploads feed concurrently, then diffs the two so any upload not in
   a playlist ends up in the `OTHER` bucket. Playlist contents fetch with
   bounded concurrency so a channel with many playlists doesn't open
   dozens of `yt-dlp` processes at once. Each channel's groups are sent
   to the UI as soon as they're ready, so results stream in instead of
   waiting for everything to finish.
3. `mpv`/`yt-dlp` run as external processes for playback/download; the
   TUI briefly steps aside (leaves the alternate screen) while they run.

## Project structure

| Path | Purpose |
|---|---|
| `src/main.rs` | Terminal setup, event loop, background task dispatch |
| `src/app.rs` | Application state machine: views, keybindings, actions |
| `src/ui.rs` | Rendering |
| `src/ytdlp.rs` | All `yt-dlp`/`mpv` subprocess orchestration |
| `src/groups.rs` | Channel → playlist/OTHER grouping logic |
| `src/cache.rs`, `src/history.rs`, `src/ustaz_list.rs`, `src/paths.rs` | Persistence |
| `src/text_field.rs` | Reusable text-input widget |
| `ustaz_list.txt` | Default curated names, embedded into the binary |

## Notes

- All content is sourced live from YouTube via `yt-dlp`; talabulilm
  itself hosts nothing.
- Respect content creators — this tool streams public YouTube videos the
  same way a browser would, it does not download/rehost anything by
  default.

## Author

Made by [Muhammad Nu'man](https://github.com/numan89).

## License

[MIT](LICENSE)
