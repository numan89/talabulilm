# talabulilm

A full-screen terminal app for watching **Ceramah Ustaz** (Islamic
lectures) — same idea as [ani-cli](https://github.com/pystardust/ani-cli),
but instead of anime it browses, groups, and streams ceramah videos, with
its own persistent UI and keybindings (like
[`cekhalal`](https://github.com/numan89/cekhalal), my other terminal
tool — same look, same feel).

Type a name or topic and it browses in a ranger-style flow:

1. **Groups** — talabulilm finds matching YouTube channels (up to a
   configurable limit — see below), and for each one lists its `PL`
   playlists plus an `OTHER` bucket for uploads the channel never put in
   a playlist. Groups stream in live as each channel finishes loading.
   Since a single query can match several channels, Groups is paged by
   channel (`n`/`p`) — each page shows one channel's playlists.
2. **Contents** — press `Enter` on a group and control moves straight
   into its video list (title + duration shown right there, no separate
   screen) rather than swapping to something else. `/` filters that
   list incrementally by title.

If no channel matches your query, it falls back to a plain flat video
search instead.

Then it plays straight into `mpv` via `yt-dlp` — no browser, no ads, no
downloads needed (unless you want one). Playback position is tracked via
mpv's IPC socket, so anything you didn't finish shows up resumable in
History.

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
talabulilm                     # launch with an empty search box
talabulilm bakhiet              # launch and immediately search
talabulilm --terminal-video     # play video as truecolor blocks in this terminal, no mpv window
talabulilm -t bakhiet           # combine with an initial search
```

By default, playing a video opens it in mpv's own window. `--terminal-video`
(short: `-t`) is an alternative for terminal-only setups (SSH, no display
server): mpv renders a small video box in the top-left corner of the
terminal itself, instead of opening a window. It's much lower fidelity than
a real window, so it stays opt-in rather than the default, and quality is
capped at 480p regardless of the selected quality — a small terminal box
can't show more detail than that anyway, so fetching more would only cost
data.

### Keybindings

| Keys | Where | Does |
|---|---|---|
| Type, `Enter` | Search | Run a search |
| `Esc` | Search | Clear the box; on an already-empty box, quit |
| `Alt+←`/`Alt+→`, `Alt+Backspace`, `Ctrl+W`, `Home`/`End` | Search | Word-jump, word-delete, clear-all, line start/end |
| `/` | anywhere else | Jump back to Search |
| `Ctrl+U` | anywhere | Browse the curated ustaz list |
| `Ctrl+H` | anywhere | Watch history |
| `Tab` | anywhere | Jump between Search and the current results/groups pane |
| `Alt+1`/`2`/`3` | anywhere | Jump to the Channels/Playlists/Uploads search limits |
| `↑`/`↓` (or `j`/`k`) | any list | Move selection |
| `n`/`p` (or PageDown/PageUp) | Groups | Next/previous channel page |
| `Enter` (or `l`) | Groups | Open the highlighted group → Contents |
| `Enter` (or `l`) | Contents/Videos/History | Play the highlighted video (resumes if History tracked a position) |
| `d` | Contents/Videos | Download instead of playing |
| `[` / `]` | Contents/Videos | Cycle quality: best, 1080p, 720p, 480p, 360p, worst, audio-only |
| `f` | History | Mark the highlighted entry finished |
| `x` | History | Delete the highlighted entry |
| `Esc` (or `h`) | any list | Go back |
| `q` / `Ctrl+C` | anywhere | Quit |

Search limits (Alt+1/2/3): `←`/`→` changes the value, `Enter` reruns the
search under it, `Tab`/`Esc` backs out without searching.

Videos you've watched to the end (or downloaded) drop out of future
search results — they're still reachable, and resumable, from `Ctrl+H`.

## Customizing the ustaz list

The first time you run talabulilm, it copies its built-in default list to
`${XDG_DATA_HOME:-$HOME/.local/share}/talabulilm/ustaz_list.txt` — edit
that copy (one name per line; `#` comments and blank lines are ignored).
`Ctrl+U` fuzzy-filters these names as you type; picking one runs the same
browse flow as a free-text search, pre-filled with that name.

## History

Watched (or downloaded) videos are logged to
`${XDG_STATE_HOME:-$HOME/.local/state}/talabulilm/history.json` so
`Ctrl+H` can bring them back up, most-recent-first, along with how far
into each one you got. Playback position is tracked live over mpv's IPC
socket, so `Enter` on a partially-watched entry resumes from where you
left off instead of starting over; `f` force-marks an entry finished and
`x` deletes it.

## Caching

Building a query's channel/playlist group listing is the slow part
(several `yt-dlp` calls). Results are cached per query under
`${XDG_DATA_HOME:-$HOME/.local/share}/talabulilm/cache/<version>/` for 7
days, so repeating the same search is instant. The cache key also
includes the current Channels/Playlists/Uploads search limits, so
changing them fetches fresh (larger or smaller) results instead of
reusing a mismatched cache; the version segment means upgrading
talabulilm never serves a stale cache in a format the new version
doesn't expect.

## How it works

talabulilm is a Rust TUI (`ratatui` + `crossterm` + `tokio`) — no server,
no database. It never talks to YouTube directly: `yt-dlp` already solves
a moving-target problem (signature ciphers, PO tokens, SABR-only
streaming) that would be a maintenance trap to reimplement, so talabulilm
just orchestrates it as a subprocess, same as `mpv` for playback.

1. `ytdlp::search_channels` asks YouTube (filtered to channel results)
   for channels matching your query, ranked by subscriber count, capped
   at the Channels limit (default 5, adjustable with Alt+1).
2. `groups::build_channel_groups` fetches each channel's playlists (up to
   the Playlists limit, Alt+2) and its uploads feed (up to the Uploads
   limit, Alt+3) concurrently, then diffs the two so any upload not in a
   playlist ends up in the `OTHER` bucket. Playlist contents fetch with
   bounded concurrency so a channel with many playlists doesn't open
   dozens of `yt-dlp` processes at once. Each channel's groups are sent
   to the UI as soon as they're ready, so results stream in instead of
   waiting for everything to finish.
3. `mpv`/`yt-dlp` run as external processes for playback/download; the
   TUI briefly steps aside (leaves the alternate screen) while they run.
   `mpv` is launched with its own IPC socket so talabulilm can observe
   `time-pos`/`duration` live and record how far you actually got.

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
