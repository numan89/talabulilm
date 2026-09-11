# talabulilm

A terminal tool for watching **Ceramah Ustaz** (Islamic lectures) — same
workflow as [ani-cli](https://github.com/pystardust/ani-cli), but instead of
anime it searches, lists, and streams ceramah videos.

Type a name or topic and it browses in two steps, like a mini video library:

1. **Pick a group** — talabulilm finds matching YouTube channels, and for
   each one lists its 📁 playlists plus a 🎬 "Other videos" bucket for
   uploads the channel never put in a playlist. A preview pane shows
   what's inside before you commit.
2. **Pick a video** — from inside the chosen channel/playlist. Press
   `Esc` here to go back to the group list (`Ctrl-C` quits entirely).

Then it streams straight into `mpv` via `yt-dlp` — no browser, no ads, no
downloads needed (unless you want one).

## Dependencies

- [`fzf`](https://github.com/junegunn/fzf)
- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp)
- [`mpv`](https://mpv.io/) (not needed if you only ever use `--download`)

Install on Arch:

```sh
sudo pacman -S fzf yt-dlp mpv
```

## Install

```sh
git clone https://github.com/numan89/talabulilm.git
cd talabulilm
chmod +x talabulilm
sudo ln -s "$PWD/talabulilm" /usr/local/bin/talabulilm
```

Or just run it in place: `./talabulilm`.

## Usage

```sh
talabulilm [options] [search terms]
```

| Option | Description |
|---|---|
| `-u, --ustaz` | Pick from a curated list of well-known ustaz first, then browse their channel(s) |
| `-c, --continue` | Rewatch something from your local history |
| `-f, --flat` | Skip channel/playlist grouping — go straight to a plain video search |
| `-d, --download` | Download instead of streaming |
| `-o, --output <dir>` | Download directory (default: current directory) |
| `-q, --quality <res>` | `best` \| `1080` \| `720` \| `480` \| `360` \| `worst` (default: `best`) |
| `-a, --audio-only` | Stream/download audio only |
| `-n, --results <num>` | Results fetched in flat search mode (default: 25) |
| `-v, --version` | Show version |
| `-h, --help` | Show help |

### Examples

```sh
talabulilm bakhiet                        # browse: channel/playlist -> video
talabulilm -u                             # pick an ustaz, then browse their channel(s)
talabulilm -f "sabar dalam ujian"         # skip grouping, plain video search
talabulilm -d -q 720 "adab menuntut ilmu" # download at 720p (flat search)
talabulilm -c                             # continue from history
```

If no channel matches your query (or with `-f`), it falls back to a plain
flat video search, same as before.

## Customizing the ustaz list

The first time you run talabulilm, it copies a default list to
`${XDG_DATA_HOME:-$HOME/.local/share}/talabulilm/ustaz_list.txt` — edit
that copy (one name per line; `#` comments and blank lines are ignored).
`talabulilm -u` fuzzy-lists these names via `fzf`; picking one runs the
same browse flow as free-text search, just pre-filled with that name.

## History

Watched videos are logged to
`${XDG_STATE_HOME:-$HOME/.local/state}/talabulilm/history.tsv` so
`talabulilm -c` can bring them back up.

## Caching

Building the channel/playlist group listing is the slow part (multiple
YouTube requests). Results are cached per query under
`${XDG_DATA_HOME:-$HOME/.local/share}/talabulilm/cache/` for 24 hours, so
repeating the same search is instant. Pass `-r`/`--refresh` to force a
re-fetch (e.g. once you know new videos were uploaded). Delete the cache
directory anytime to clear it entirely.

## How it works

talabulilm is a single bash script; there's no server or database.

1. `search_channels` asks YouTube (via `yt-dlp`, filtered to channel results)
   for channels matching your query, ranked by subscriber count.
2. `build_groups` fetches each channel's playlists and its uploads feed, then
   diffs the two so any upload not in a playlist ends up in an "Other videos"
   bucket. Playlist contents are fetched in parallel (capped at 5 at a time)
   to keep this reasonably fast.
3. Two `fzf` prompts (group, then video) drive selection; `mpv` (with
   `yt-dlp` as its extractor backend) handles playback.

## Project structure

| File | Purpose |
|---|---|
| `talabulilm` | The script — everything lives here |
| `ustaz_list.txt` | Curated names shown by `-u`; edit freely |
| `README.md` | This file |
| `LICENSE` | MIT license |

## Notes

- All content is sourced live from YouTube via `yt-dlp`; talabulilm itself
  hosts nothing.
- Respect content creators — this tool streams public YouTube videos the
  same way a browser would, it does not download/rehost anything by default.

## License

[MIT](LICENSE)
