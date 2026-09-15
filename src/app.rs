use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use crate::groups::GroupRow;
use crate::history::{self, HistoryEntry};
use crate::text_field::TextField;
use crate::ustaz_list;
use crate::ytdlp::{Quality, Video};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Search,
    Groups,
    Results,
    UstazList,
    History,
}

/// Which of Groups/Results the main pane shows while `view == Search` —
/// same idea as cekhalal keeping its Results pane visible behind the
/// Search box: focusing Search doesn't blank out whatever was last
/// searched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentPane {
    Groups,
    Results,
}

/// How many channels/playlists/uploads a search pulls in. Shown as three
/// small boxes next to Search, reachable with Alt+1/2/3 — same layout and
/// binding as cekhalal's Mode/State/Category filters — rather than tucked
/// away behind a separate screen. The defaults mirror the old hardcoded
/// constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchLimits {
    pub channels: u32,
    pub playlists: u32,
    pub uploads: u32,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self { channels: 5, playlists: 15, uploads: 60 }
    }
}

const CHANNELS_RANGE: (u32, u32) = (1, 15);
const PLAYLISTS_RANGE: (u32, u32) = (0, 40);
const UPLOADS_RANGE: (u32, u32) = (10, 200);

impl SearchLimits {
    fn field(&mut self, index: usize) -> (&mut u32, u32, (u32, u32)) {
        match index {
            0 => (&mut self.channels, 1, CHANNELS_RANGE),
            1 => (&mut self.playlists, 5, PLAYLISTS_RANGE),
            _ => (&mut self.uploads, 10, UPLOADS_RANGE),
        }
    }

    fn adjust(&mut self, index: usize, delta: i32) {
        let (value, step, (min, max)) = self.field(index);
        let step = step as i32;
        let next = (*value as i32 + delta * step).clamp(min as i32, max as i32);
        *value = next as u32;
    }
}

/// What the caller (main.rs) should do after a key press. Keeps App free
/// of any knowledge of tokio/yt-dlp/mpv — it just describes intent.
#[derive(Debug, Clone)]
pub enum Action {
    None,
    Quit,
    RunSearch(u64, String, SearchLimits),
    /// Second field is a resume position in seconds, from a History
    /// entry's tracked progress — `None` plays from the start.
    Play(Video, Option<f64>),
    Download(Video),
}

pub enum AppEvent {
    /// Tagged with the generation it was requested under, so a slow
    /// response for a search the user has since refined can be dropped
    /// instead of clobbering newer results. Sent for the *flat* fallback
    /// path (no channel matched the query, or the channel search itself
    /// failed).
    SearchResult(u64, anyhow::Result<Vec<Video>>),
    /// Channel search succeeded and grouping is starting: how many
    /// channels to expect, so the UI can show "loaded X/Y".
    ChannelsFound(u64, usize),
    /// One channel's groups finished building (playlists + OTHER bucket).
    /// Sent progressively, one per channel, as each completes.
    ChannelGroupsReady(u64, anyhow::Result<Vec<GroupRow>>),
}

pub struct App {
    pub input: TextField,
    pub view: View,
    pub content_pane: ContentPane,
    pub quality: Quality,
    /// Set once at startup from `--terminal-video`/`-t`: selecting
    /// something to play hands it off to a detached, audio-only
    /// background process (see `spawn_play_background` in `main.rs`) and
    /// quits immediately instead of taking over the terminal, so the
    /// window this was launched in just closes while playback continues.
    /// Default stays the normal foreground/windowed `play` — this is an
    /// opt-in alternative, not a replacement.
    pub terminal_video: bool,
    pub limits: SearchLimits,
    /// Which of the three limit boxes (Channels/Playlists/Uploads) has
    /// focus, if any — `None` means the search row is either on the
    /// Search box itself or, if `view` isn't `Search`, not on the row at
    /// all. Alt+1/2/3 sets this from anywhere, same as cekhalal's
    /// Mode/State/Category jumps.
    pub limits_focus: Option<usize>,

    pub results: Vec<Video>,
    pub list_state: ListState,

    pub groups: Vec<GroupRow>,
    pub groups_list_state: ListState,
    pub channels_total: usize,
    pub channels_loaded: usize,
    /// Groups is paged by *channel* (n/p, same binding as cekhalal's
    /// Results paging) rather than by raw group row — each page shows
    /// just one matched channel's playlists + OTHER bucket, since a flat
    /// list mixing several channels' groups together is what the
    /// grouping exists to avoid in the first place.
    pub channel_page: usize,
    /// Indices into `groups` belonging to the current channel page,
    /// recomputed whenever the page changes or new channel data arrives.
    pub channel_groups: Vec<usize>,
    /// Whether the Groups view's keyboard control is on the Contents
    /// pane (the selected group's videos, right side) rather than the
    /// group list itself. Enter on a group moves focus here directly —
    /// no separate full-screen "Videos" view needed just to pick and
    /// play one, since Contents already shows everything (title,
    /// duration) needed to choose.
    pub contents_focused: bool,
    pub contents_list_state: ListState,
    /// Incremental filter over the *currently selected* group's video
    /// list — same idea as cekhalal's product filter over a company's
    /// products. `contents_filtered` holds indices into that group's
    /// `videos`, recomputed whenever the filter text or the selected
    /// group changes.
    pub contents_filter: TextField,
    pub contents_filter_active: bool,
    pub contents_filtered: Vec<usize>,

    pub ustaz_names: Vec<String>,
    pub ustaz_filter: TextField,
    pub ustaz_filtered: Vec<usize>,
    pub ustaz_list_state: ListState,

    pub history: Vec<HistoryEntry>,
    pub history_list_state: ListState,

    pub loading: bool,
    pub error: Option<String>,
    pub status: Option<String>,
    search_generation: u64,
    /// Whether a search has ever actually completed — distinct from
    /// `status` (which carries unrelated messages like "Finished
    /// playing: X"), so the Search screen's placeholder can tell "never
    /// searched, show the hint" apart from "searched, found nothing"
    /// instead of losing the hint the first time something else sets
    /// `status`. Same idea as cekhalal's own `searched_once`.
    pub searched_once: bool,

    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        ustaz_list::ensure_seeded();
        let ustaz_names = ustaz_list::load();
        let ustaz_filtered = (0..ustaz_names.len()).collect();

        Self {
            input: TextField::new(),
            view: View::Search,
            content_pane: ContentPane::Results,
            quality: Quality::Best,
            terminal_video: false,
            limits: SearchLimits::default(),
            limits_focus: None,
            results: Vec::new(),
            list_state: ListState::default(),
            groups: Vec::new(),
            groups_list_state: ListState::default(),
            channels_total: 0,
            channels_loaded: 0,
            channel_page: 0,
            channel_groups: Vec::new(),
            contents_focused: false,
            contents_list_state: ListState::default(),
            contents_filter: TextField::new(),
            contents_filter_active: false,
            contents_filtered: Vec::new(),
            ustaz_names,
            ustaz_filter: TextField::new(),
            ustaz_filtered,
            ustaz_list_state: ListState::default(),
            history: Vec::new(),
            history_list_state: ListState::default(),
            loading: false,
            error: None,
            status: None,
            search_generation: 0,
            searched_once: false,
            should_quit: false,
        }
    }

    pub fn selected_video(&self) -> Option<&Video> {
        self.list_state.selected().and_then(|i| self.results.get(i))
    }

    pub fn selected_group(&self) -> Option<&GroupRow> {
        let sel = self.groups_list_state.selected()?;
        let idx = *self.channel_groups.get(sel)?;
        self.groups.get(idx)
    }

    /// Distinct channel names among `groups`, in order of first
    /// appearance — the page order for Groups' n/p channel paging.
    fn channel_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for g in &self.groups {
            if !names.iter().any(|n| n == &g.channel) {
                names.push(g.channel.clone());
            }
        }
        names
    }

    pub fn channel_page_count(&self) -> usize {
        self.channel_names().len()
    }

    pub fn current_channel_name(&self) -> Option<String> {
        self.channel_names().into_iter().nth(self.channel_page)
    }

    /// Recomputes which of `groups` belong to the current channel page,
    /// preserving the row selection where it's still in range — used
    /// when new channel data streams in in the background, which
    /// shouldn't disturb browsing already under way on some other page.
    fn refresh_channel_page(&mut self) {
        let names = self.channel_names();
        if let Some(name) = names.get(self.channel_page) {
            self.channel_groups = self.groups.iter().enumerate().filter(|(_, g)| &g.channel == name).map(|(i, _)| i).collect();
        } else {
            self.channel_page = 0;
            self.channel_groups.clear();
        }
        let len = self.channel_groups.len();
        let stays_valid = matches!(self.groups_list_state.selected(), Some(i) if i < len);
        if !stays_valid {
            self.groups_list_state.select(if len > 0 { Some(0) } else { None });
        }
    }

    /// Explicit page change (n/p) — always lands on the new page's first
    /// group, and resets Contents the same way selecting a different
    /// group within a page already does.
    fn move_channel_page(&mut self, delta: i32) {
        let total = self.channel_page_count();
        if total == 0 {
            return;
        }
        let total = total as i32;
        self.channel_page = (self.channel_page as i32 + delta).rem_euclid(total) as usize;
        self.refresh_channel_page();
        self.groups_list_state.select(if self.channel_groups.is_empty() { None } else { Some(0) });
        self.contents_focused = false;
        self.contents_filter.clear();
        self.refresh_contents_filter();
    }

    pub fn selected_group_video(&self) -> Option<&Video> {
        let group = self.selected_group()?;
        let sel = self.contents_list_state.selected()?;
        let idx = *self.contents_filtered.get(sel)?;
        group.videos.get(idx)
    }

    /// Recomputes which of the selected group's videos match the current
    /// filter text (all of them, if the filter is empty), and keeps the
    /// selection sane against that new set.
    fn refresh_contents_filter(&mut self) {
        let needle = self.contents_filter.as_string().to_lowercase();
        self.contents_filtered = match self.selected_group() {
            Some(group) => {
                group.videos.iter().enumerate().filter(|(_, v)| needle.is_empty() || v.title.to_lowercase().contains(&needle)).map(|(i, _)| i).collect()
            }
            None => Vec::new(),
        };
        self.contents_list_state.select(if self.contents_filtered.is_empty() { None } else { Some(0) });
    }

    pub fn selected_ustaz(&self) -> Option<&str> {
        self.ustaz_list_state.selected().and_then(|i| self.ustaz_filtered.get(i)).and_then(|&idx| self.ustaz_names.get(idx)).map(String::as_str)
    }

    pub fn selected_history(&self) -> Option<&HistoryEntry> {
        self.history_list_state.selected().and_then(|i| self.history.get(i))
    }

    pub fn apply_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::SearchResult(generation, result) => {
                if generation != self.search_generation {
                    return; // stale: a newer search has already started
                }
                self.loading = false;
                self.searched_once = true;
                match result {
                    Ok(videos) => {
                        self.error = None;
                        self.view = View::Results;
                        self.content_pane = ContentPane::Results;
                        let finished = history::finished_ids();
                        let videos: Vec<Video> = videos.into_iter().filter(|v| !finished.contains(&v.id)).collect();
                        self.list_state.select(if videos.is_empty() { None } else { Some(0) });
                        self.results = videos;
                    }
                    Err(e) => self.error = Some(format!("{e:#}")),
                }
            }
            AppEvent::ChannelsFound(generation, total) => {
                if generation != self.search_generation {
                    return;
                }
                self.loading = false;
                self.error = None;
                self.searched_once = true;
                self.view = View::Groups;
                self.content_pane = ContentPane::Groups;
                self.groups.clear();
                self.groups_list_state.select(None);
                self.channel_page = 0;
                self.channel_groups.clear();
                self.contents_focused = false;
                self.contents_filter.clear();
                self.contents_filter_active = false;
                self.contents_filtered.clear();
                self.contents_list_state.select(None);
                self.channels_total = total;
                self.channels_loaded = 0;
            }
            AppEvent::ChannelGroupsReady(generation, result) => {
                if generation != self.search_generation {
                    return;
                }
                self.channels_loaded += 1;
                if let Ok(rows) = result {
                    let finished = history::finished_ids();
                    let rows: Vec<GroupRow> = rows
                        .into_iter()
                        .map(|mut g| {
                            g.videos.retain(|v| !finished.contains(&v.id));
                            g
                        })
                        .filter(|g| !g.videos.is_empty())
                        .collect();
                    self.groups.extend(rows);
                    self.refresh_channel_page();
                    // Only auto-sync Contents when the user isn't already
                    // browsing it — a channel finishing in the background
                    // shouldn't reset whatever they're looking at.
                    if !self.contents_focused {
                        self.refresh_contents_filter();
                    }
                }
            }
        }
    }

    pub fn trigger_search(&mut self) -> Action {
        let query = self.input.as_string();
        if query.trim().is_empty() {
            return Action::None;
        }
        self.search_generation += 1;
        self.loading = true;
        self.error = None;
        self.status = None;
        self.groups.clear();
        self.results.clear();
        self.view = View::Search;
        Action::RunSearch(self.search_generation, query, self.limits)
    }

    /// Pre-fills the search box from a CLI argument and immediately
    /// triggers a search, keeping `talabulilm <query>` muscle memory from
    /// the old bash version.
    pub fn seed_query(&mut self, query: &str) -> Action {
        for c in query.chars() {
            self.input.insert_char(c);
        }
        self.trigger_search()
    }

    /// Runs a search for a name picked from the UstazList view: replaces
    /// whatever was in the search box (same behavior as the bash
    /// version's `-u`, which pre-fills the query with the picked name).
    fn run_query(&mut self, query: &str) -> Action {
        self.input.clear();
        for c in query.chars() {
            self.input.insert_char(c);
        }
        self.trigger_search()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Action::Quit;
        }

        // Channels/Playlists/Uploads are one Alt+digit away from
        // anywhere, so the main Tab cycle can stay simple without also
        // having to pass through these — same pattern as cekhalal's own
        // Alt+1/2/3 Mode/State/Category jumps.
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Char('1') => {
                    self.limits_focus = Some(0);
                    return Action::None;
                }
                KeyCode::Char('2') => {
                    self.limits_focus = Some(1);
                    return Action::None;
                }
                KeyCode::Char('3') => {
                    self.limits_focus = Some(2);
                    return Action::None;
                }
                _ => {}
            }
        }

        // `/` jumps back to the Search box (same as cekhalal's `/`
        // binding) — checked ahead of the limits-focus routing below so
        // it still works while a Channels/Playlists/Uploads box has
        // focus (resetting that focus back to Search too), not just
        // from a plain view.
        let limits_free = self.limits_focus.is_none();
        let contents_focused = self.view == View::Groups && self.contents_focused;
        // '/' is blocked where a typed '/' would land in a text field,
        // plus whenever Contents has focus at all (filtering or not) —
        // it owns '/' itself there to start its own filter, same
        // asymmetry as cekhalal's Preview pane.
        let blocks_slash = limits_free && (matches!(self.view, View::Search | View::UstazList) || contents_focused);
        if key.code == KeyCode::Char('/') && !blocks_slash {
            self.limits_focus = None;
            self.view = View::Search;
            return Action::None;
        }

        if let Some(field) = self.limits_focus {
            return self.handle_key_limits_focus(key, field);
        }

        // Global, reachable from any view — otherwise a view's own "go
        // back" binding on a bare Char('h') would swallow Ctrl+H first.
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('u') => {
                    self.ustaz_filter.clear();
                    self.refresh_ustaz_filter();
                    self.view = View::UstazList;
                    return Action::None;
                }
                KeyCode::Char('h') => {
                    self.history = history::load();
                    self.history_list_state.select(if self.history.is_empty() { None } else { Some(0) });
                    self.view = View::History;
                    return Action::None;
                }
                _ => {}
            }
        }

        match self.view {
            View::Search => self.handle_key_search(key),
            View::Groups => self.handle_key_groups(key),
            View::Results => self.handle_key_results(key),
            View::UstazList => self.handle_key_ustaz(key),
            View::History => self.handle_key_history(key),
        }
    }

    /// Handles a key while one of the Channels/Playlists/Uploads boxes
    /// has focus — mirrors cekhalal's Mode/State/Category filter
    /// handlers: it fully owns the key (nothing falls through to the
    /// underlying view) until Esc/Tab/Enter hands focus back.
    fn handle_key_limits_focus(&mut self, key: KeyEvent, field: usize) -> Action {
        match key.code {
            KeyCode::Esc | KeyCode::Tab => {
                self.limits_focus = None;
                self.view = View::Search;
            }
            // Same pattern as cekhalal's Mode/State/Category filter:
            // ←/→ only edits the value, Enter is what actually commits
            // it by rerunning the search under the new limits.
            KeyCode::Enter => {
                self.limits_focus = None;
                return self.trigger_search();
            }
            KeyCode::Left | KeyCode::Char('h') => self.limits.adjust(field, -1),
            KeyCode::Right | KeyCode::Char('l') => self.limits.adjust(field, 1),
            _ => {}
        }
        Action::None
    }

    fn handle_key_search(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Enter => return self.trigger_search(),
            // Search is the top of the view stack (every other view's Esc
            // already lands back here) — same "Esc to leave" chain as
            // cekhalal's Focus::Results, just one level further out: a
            // non-empty box clears first, an empty one quits.
            KeyCode::Esc => {
                if self.input.is_empty() {
                    return Action::Quit;
                }
                self.input.clear();
                return Action::None;
            }
            // Always switches to the content pane, empty or not — same as
            // cekhalal's Tab, which happily lands on an empty Results
            // list rather than refusing to switch until there's data.
            KeyCode::Tab => {
                self.view = match self.content_pane {
                    ContentPane::Groups => View::Groups,
                    ContentPane::Results => View::Results,
                };
                return Action::None;
            }
            _ => {}
        }
        self.input.handle_key(key);
        Action::None
    }

    fn handle_key_groups(&mut self, key: KeyEvent) -> Action {
        if self.contents_focused {
            return self.handle_key_group_contents(key);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Tab | KeyCode::Char('h') => self.view = View::Search,
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') => self.move_group_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_group_selection(-1),
            // n/p page between channels, same binding as cekhalal's own
            // Results paging.
            KeyCode::PageDown | KeyCode::Char('n') => self.move_channel_page(1),
            KeyCode::PageUp | KeyCode::Char('p') => self.move_channel_page(-1),
            // Moves control into the Contents pane rather than swapping
            // to a separate full-screen Videos view — Contents already
            // shows title + duration for every video in the group, so
            // there's nothing a dedicated screen would add.
            KeyCode::Enter | KeyCode::Char('l') => {
                if let Some(group) = self.selected_group()
                    && !group.videos.is_empty()
                {
                    self.contents_focused = true;
                    if self.contents_filtered.is_empty() {
                        self.refresh_contents_filter();
                    }
                }
            }
            _ => {}
        }
        Action::None
    }

    fn handle_key_group_contents(&mut self, key: KeyEvent) -> Action {
        // Same shape as cekhalal's Preview product filter: while typing
        // the filter, everything goes to the text field except Esc/Enter
        // to leave typing mode (the filter itself stays applied).
        if self.contents_filter_active {
            match key.code {
                KeyCode::Esc => {
                    self.contents_filter_active = false;
                    self.contents_filter.clear();
                    self.refresh_contents_filter();
                }
                KeyCode::Enter => self.contents_filter_active = false,
                _ => {
                    if self.contents_filter.handle_key(key) {
                        self.refresh_contents_filter();
                    }
                }
            }
            return Action::None;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('h') => {
                if !self.contents_filter.is_empty() {
                    self.contents_filter.clear();
                    self.refresh_contents_filter();
                } else {
                    self.contents_focused = false;
                }
            }
            KeyCode::Char('/') => self.contents_filter_active = true,
            KeyCode::Tab => {
                self.contents_focused = false;
                self.view = View::Search;
            }
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') => self.move_content_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_content_selection(-1),
            KeyCode::Enter | KeyCode::Char('l') => {
                if let Some(v) = self.selected_group_video() {
                    return Action::Play(v.clone(), None);
                }
            }
            KeyCode::Char('d') => {
                if let Some(v) = self.selected_group_video() {
                    return Action::Download(v.clone());
                }
            }
            KeyCode::Char('[') => self.quality = cycle_prev(self.quality),
            KeyCode::Char(']') => self.quality = self.quality.cycle_next(),
            _ => {}
        }
        Action::None
    }

    fn handle_key_results(&mut self, key: KeyEvent) -> Action {
        match key.code {
            // Results is only ever entered here from Search now (drilling
            // into a group's videos moves into Contents within Groups
            // instead), so there's nowhere else to back out to.
            KeyCode::Esc | KeyCode::Char('h') => self.view = View::Search,
            KeyCode::Tab => self.view = View::Search,
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Enter | KeyCode::Char('l') => {
                if let Some(v) = self.selected_video() {
                    return Action::Play(v.clone(), None);
                }
            }
            KeyCode::Char('d') => {
                if let Some(v) = self.selected_video() {
                    return Action::Download(v.clone());
                }
            }
            KeyCode::Char('[') => self.quality = cycle_prev(self.quality),
            KeyCode::Char(']') => self.quality = self.quality.cycle_next(),
            _ => {}
        }
        Action::None
    }

    fn handle_key_ustaz(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => self.view = View::Search,
            KeyCode::Char('q') if self.ustaz_filter.is_empty() => return Action::Quit,
            KeyCode::Down => self.move_ustaz_selection(1),
            KeyCode::Up => self.move_ustaz_selection(-1),
            KeyCode::Enter => {
                if let Some(name) = self.selected_ustaz().map(str::to_string) {
                    return self.run_query(&name);
                }
            }
            _ => {
                if self.ustaz_filter.handle_key(key) {
                    self.refresh_ustaz_filter();
                }
            }
        }
        Action::None
    }

    fn handle_key_history(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') => self.view = View::Search,
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') => self.move_history_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_history_selection(-1),
            KeyCode::Enter | KeyCode::Char('l') => {
                if let Some(entry) = self.selected_history() {
                    let resume_from = entry.resume_from();
                    return Action::Play(entry.to_video(), resume_from);
                }
            }
            KeyCode::Char('f') => {
                if let Some(entry) = self.selected_history() {
                    history::mark_finished(&entry.id);
                    self.refresh_history();
                }
            }
            KeyCode::Char('x') => {
                if let Some(entry) = self.selected_history() {
                    let id = entry.id.clone();
                    history::remove(&id);
                    self.refresh_history();
                }
            }
            _ => {}
        }
        Action::None
    }

    /// Reloads history from disk and keeps the selection sane afterward
    /// (deleting the last entry, or the last one on screen, shouldn't
    /// leave the cursor pointing past the end of the list). Also called
    /// from main.rs after a play/resume finishes, so a just-recorded
    /// watch shows up immediately in the History view instead of only
    /// after leaving and re-entering it (which is the only other place
    /// that reloads from disk).
    pub fn refresh_history(&mut self) {
        self.history = history::load();
        let len = self.history.len();
        match self.history_list_state.selected() {
            Some(_) if len == 0 => self.history_list_state.select(None),
            Some(i) if i >= len => self.history_list_state.select(Some(len - 1)),
            None if len > 0 => self.history_list_state.select(Some(0)),
            _ => {}
        }
    }

    fn refresh_ustaz_filter(&mut self) {
        let needle = self.ustaz_filter.as_string().to_lowercase();
        self.ustaz_filtered =
            self.ustaz_names.iter().enumerate().filter(|(_, name)| needle.is_empty() || name.to_lowercase().contains(&needle)).map(|(i, _)| i).collect();
        self.ustaz_list_state.select(if self.ustaz_filtered.is_empty() { None } else { Some(0) });
    }

    fn move_selection(&mut self, delta: i32) {
        move_list_selection(&mut self.list_state, self.results.len(), delta);
    }

    fn move_group_selection(&mut self, delta: i32) {
        move_list_selection(&mut self.groups_list_state, self.channel_groups.len(), delta);
        // Contents follows whichever group is now highlighted, so its
        // own filter/selection doesn't carry over from the previous
        // group — same as cekhalal clearing its product filter when the
        // highlighted company/product changes.
        self.contents_filter.clear();
        self.refresh_contents_filter();
    }

    fn move_content_selection(&mut self, delta: i32) {
        move_list_selection(&mut self.contents_list_state, self.contents_filtered.len(), delta);
    }

    fn move_ustaz_selection(&mut self, delta: i32) {
        move_list_selection(&mut self.ustaz_list_state, self.ustaz_filtered.len(), delta);
    }

    fn move_history_selection(&mut self, delta: i32) {
        move_list_selection(&mut self.history_list_state, self.history.len(), delta);
    }
}

fn move_list_selection(state: &mut ListState, len: usize, delta: i32) {
    if len == 0 {
        return;
    }
    let len = len as i32;
    let current = state.selected().unwrap_or(0) as i32;
    let next = (current + delta).rem_euclid(len);
    state.select(Some(next as usize));
}

fn cycle_prev(q: Quality) -> Quality {
    // 7 variants total; cycling forward 6 times is cycling back once.
    let mut q = q;
    for _ in 0..6 {
        q = q.cycle_next();
    }
    q
}
