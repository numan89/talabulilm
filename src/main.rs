mod app;
mod cache;
mod groups;
mod history;
mod paths;
mod text_field;
mod ui;
mod ustaz_list;
mod ytdlp;

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use app::{Action, App, AppEvent, SearchLimits};
use ytdlp::{Quality, Video};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SEARCH_COUNT: u32 = 25; // flat-search fallback result count
const PARALLEL_JOBS: usize = 5; // concurrent yt-dlp calls per channel's playlists

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn init_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Hidden entry point for `-t`'s "close the terminal, keep playing"
    // flow: spawned detached (setsid, no controlling terminal - see
    // `spawn_play_background` below) by the interactive TUI right before
    // it quits, so this runs on as its own independent process and is
    // still around to record history once mpv actually exits.
    if args.first().map(String::as_str) == Some("--play-bg") {
        let id = args.get(1).cloned().unwrap_or_default();
        let title = args.get(2).cloned().unwrap_or_default();
        let channel = args.get(3).cloned().unwrap_or_default();
        let resume_from = args.get(4).and_then(|s| s.parse::<f64>().ok());
        let video = Video { id: id.clone(), title: title.clone(), channel, duration: String::new() };
        match ytdlp::play_audio_background(&id, &title, resume_from).await {
            Ok(outcome) => history::record_watch(&video, outcome),
            Err(e) => eprintln!("talabulilm --play-bg: {e:#}"),
        }
        return Ok(());
    }

    let terminal_video = args.iter().any(|a| a == "--terminal-video" || a == "-t");
    let initial_query = args
        .into_iter()
        .filter(|a| a != "--terminal-video" && a != "-t")
        .collect::<Vec<_>>()
        .join(" ");

    let mut terminal = init_terminal()?;
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut app = App::new();
    app.terminal_video = terminal_video;

    if !initial_query.trim().is_empty() {
        let action = app.seed_query(&initial_query);
        dispatch(action, &tx);
    }

    let result = run(&mut terminal, &mut app, tx, &mut rx).await;

    restore_terminal()?;
    terminal.show_cursor()?;

    if let Err(err) = &result {
        eprintln!("talabulilm exited with an error: {err:#}");
    }
    result
}

async fn run(
    terminal: &mut Tui,
    app: &mut App,
    tx: mpsc::UnboundedSender<AppEvent>,
    rx: &mut mpsc::UnboundedReceiver<AppEvent>,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let action = app.handle_key(key);
                    handle_action(action, app, terminal, &tx).await?;
                }
            }
        }

        while let Ok(evt) = rx.try_recv() {
            app.apply_event(evt);
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

/// Play/Download need to temporarily give the terminal back (mpv/yt-dlp
/// expect a normal terminal, not our raw-mode alternate screen), so they
/// run right here rather than through the generic background-task
/// dispatch below.
async fn handle_action(
    action: Action,
    app: &mut App,
    terminal: &mut Tui,
    tx: &mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    match action {
        Action::Quit => app.should_quit = true,
        Action::Play(video, resume_from) => {
            if app.terminal_video {
                // -t's "pick something, close the terminal" flow: hand
                // playback to a detached background process (see
                // `--play-bg` above) instead of taking over the
                // terminal ourselves, then quit outright so the kitty
                // window this was launched in closes on its own -
                // there's no foreground playback left to wait for or
                // history to record here, the detached process does
                // both once mpv actually exits.
                spawn_play_background(&video, resume_from)?;
                app.should_quit = true;
            } else {
                let title = video.title.clone();
                let quality = app.quality;
                let outcome = suspend_for_external(terminal, || play_blocking(video.clone(), quality, resume_from))?;
                match outcome {
                    Some(outcome) => {
                        history::record_watch(&video, outcome);
                        // Otherwise the History view (if that's where this
                        // play was launched from) keeps showing the stale
                        // pre-playback progress until you leave and re-enter
                        // it, since that's the only other place it reloads
                        // from disk.
                        app.refresh_history();
                        app.status = Some(format!("Finished playing: {title}"));
                    }
                    None => app.status = Some(format!("Playback failed: {title}")),
                }
            }
        }
        Action::Download(video) => {
            let title = video.title.clone();
            let quality = app.quality;
            let outcome = suspend_for_external(terminal, || download_blocking(video.clone(), quality))?;
            if outcome.is_some() {
                history::record_watch(&video, ytdlp::PlaybackOutcome { position_secs: None, duration_secs: None, finished: true });
                app.refresh_history();
                app.status = Some(format!("Downloaded: {title}"));
            } else {
                app.status = Some(format!("Download failed: {title}"));
            }
        }
        other => dispatch(other, tx),
    }
    Ok(())
}

/// Gives the terminal back to mpv/yt-dlp for the duration of `f`, then
/// restores our alternate-screen TUI. `f`'s error (if any) is printed
/// rather than propagated — a failed play/download shouldn't crash the
/// whole app — so the caller gets `None` to distinguish that from success.
fn suspend_for_external<T>(terminal: &mut Tui, f: impl FnOnce() -> Result<T>) -> Result<Option<T>> {
    restore_terminal()?;
    let result = f();
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;
    match result {
        Ok(v) => Ok(Some(v)),
        Err(e) => {
            eprintln!("talabulilm: {e:#}");
            Ok(None)
        }
    }
}

fn play_blocking(video: Video, quality: Quality, resume_from: Option<f64>) -> Result<ytdlp::PlaybackOutcome> {
    let rt = tokio::runtime::Handle::current();
    let url = format!("https://www.youtube.com/watch?v={}", video.id);
    println!("Now playing: {}", video.title);
    tokio::task::block_in_place(|| rt.block_on(ytdlp::play(&url, &video.title, quality, resume_from)))
}

/// Hands `video` off to a `--play-bg` child (see the top of `main` above)
/// fully detached from this process and its terminal: `setsid` puts it in
/// its own session so closing the terminal window (which would otherwise
/// SIGHUP the whole process group) can't touch it, and the null stdio
/// means it needs nothing back from us once spawned.
fn spawn_play_background(video: &Video, resume_from: Option<f64>) -> Result<()> {
    let exe = std::env::current_exe().context("failed to resolve talabulilm's own path")?;
    let mut cmd = std::process::Command::new("setsid");
    cmd.arg(exe).arg("--play-bg").arg(&video.id).arg(&video.title).arg(&video.channel);
    if let Some(pos) = resume_from {
        cmd.arg(pos.to_string());
    }
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd.spawn().context("failed to detach background playback (is `setsid` installed?)")?;
    Ok(())
}

fn download_blocking(video: Video, quality: Quality) -> Result<()> {
    let rt = tokio::runtime::Handle::current();
    let url = format!("https://www.youtube.com/watch?v={}", video.id);
    let dir = std::env::current_dir()?.to_string_lossy().to_string();
    println!("Downloading: {}", video.title);
    tokio::task::block_in_place(|| rt.block_on(ytdlp::download(&url, &dir, quality)))
}

fn dispatch(action: Action, tx: &mpsc::UnboundedSender<AppEvent>) {
    match action {
        Action::None | Action::Play(..) | Action::Download(_) | Action::Quit => {}
        Action::RunSearch(generation, query, limits) => {
            let tx = tx.clone();
            tokio::spawn(run_search(generation, query, limits, tx));
        }
    }
}

/// The full search pipeline for one query: try the cache, else find
/// matching channels and group their playlists (sending progress events
/// as each channel finishes), falling back to a flat keyword search if no
/// channel matches at all.
async fn run_search(generation: u64, query: String, limits: SearchLimits, tx: mpsc::UnboundedSender<AppEvent>) {
    let cache_path = cache::path_for(&paths::data_dir(), VERSION, &query, &limits);
    if let Some(cached) = cache::load(&cache_path) {
        let _ = tx.send(AppEvent::ChannelsFound(generation, 1));
        let _ = tx.send(AppEvent::ChannelGroupsReady(generation, Ok(cached)));
        return;
    }

    let channels = match ytdlp::search_channels(&query, limits.channels).await {
        Ok(c) if !c.is_empty() => c,
        _ => {
            let res = ytdlp::search_videos(&query, SEARCH_COUNT).await;
            let _ = tx.send(AppEvent::SearchResult(generation, res));
            return;
        }
    };

    let _ = tx.send(AppEvent::ChannelsFound(generation, channels.len()));

    let mut handles = Vec::new();
    for channel in channels {
        let tx = tx.clone();
        handles.push(tokio::spawn(async move {
            let result = groups::build_channel_groups(channel, limits.uploads, limits.playlists, PARALLEL_JOBS).await;
            let for_cache = match &result {
                Ok(rows) => Some(rows.clone()),
                Err(_) => None,
            };
            let _ = tx.send(AppEvent::ChannelGroupsReady(generation, result));
            for_cache
        }));
    }

    let mut all_rows = Vec::new();
    for handle in handles {
        if let Ok(Some(rows)) = handle.await {
            all_rows.extend(rows);
        }
    }
    if !all_rows.is_empty() {
        let _ = cache::save(&cache_path, &all_rows);
    }
}
