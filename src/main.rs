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

use anyhow::Result;
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
    let initial_query = std::env::args().skip(1).collect::<Vec<_>>().join(" ");

    let mut terminal = init_terminal()?;
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut app = App::new();

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
