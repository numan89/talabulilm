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

/// What the caller (main.rs) should do after a key press. Keeps App free
/// of any knowledge of tokio/yt-dlp/mpv — it just describes intent.
#[derive(Debug, Clone)]
pub enum Action {
    None,
    Quit,
    RunSearch(u64, String),
    Play(Video),
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
    /// Where Esc in the Results view should return to: Search (flat
    /// search) or Groups (drilled into a group's videos).
    results_back_view: View,
    pub quality: Quality,

    pub results: Vec<Video>,
    pub list_state: ListState,

    pub groups: Vec<GroupRow>,
    pub groups_list_state: ListState,
    pub channels_total: usize,
    pub channels_loaded: usize,

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
            results_back_view: View::Search,
            quality: Quality::Best,
            results: Vec::new(),
            list_state: ListState::default(),
            groups: Vec::new(),
            groups_list_state: ListState::default(),
            channels_total: 0,
            channels_loaded: 0,
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
            should_quit: false,
        }
    }

    pub fn selected_video(&self) -> Option<&Video> {
        self.list_state.selected().and_then(|i| self.results.get(i))
    }

    pub fn selected_group(&self) -> Option<&GroupRow> {
        self.groups_list_state.selected().and_then(|i| self.groups.get(i))
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
                match result {
                    Ok(videos) => {
                        self.error = None;
                        self.results_back_view = View::Search;
                        self.view = View::Results;
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
                self.view = View::Groups;
                self.groups.clear();
                self.groups_list_state.select(None);
                self.channels_total = total;
                self.channels_loaded = 0;
            }
            AppEvent::ChannelGroupsReady(generation, result) => {
                if generation != self.search_generation {
                    return;
                }
                self.channels_loaded += 1;
                if let Ok(rows) = result {
                    let was_empty = self.groups.is_empty();
                    self.groups.extend(rows);
                    if was_empty && !self.groups.is_empty() {
                        self.groups_list_state.select(Some(0));
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
        Action::RunSearch(self.search_generation, query)
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

        // Global, reachable from any view (same pattern cekhalal uses for
        // its Alt+1/2/3 filter jumps) — otherwise a view's own "go back"
        // binding on a bare Char('h') would swallow Ctrl+H first.
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

    fn handle_key_search(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Enter => return self.trigger_search(),
            KeyCode::Tab => {
                if !self.groups.is_empty() {
                    self.view = View::Groups;
                } else if !self.results.is_empty() {
                    self.view = View::Results;
                }
                return Action::None;
            }
            _ => {}
        }
        self.input.handle_key(key);
        Action::None
    }

    fn handle_key_groups(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc | KeyCode::Tab | KeyCode::Char('h') => self.view = View::Search,
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') => self.move_group_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_group_selection(-1),
            KeyCode::Enter | KeyCode::Char('l') => {
                if let Some(group) = self.selected_group() {
                    self.results = group.videos.clone();
                    self.list_state.select(if self.results.is_empty() { None } else { Some(0) });
                    self.results_back_view = View::Groups;
                    self.view = View::Results;
                }
            }
            _ => {}
        }
        Action::None
    }

    fn handle_key_results(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') => self.view = self.results_back_view,
            KeyCode::Tab => self.view = View::Search,
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Enter | KeyCode::Char('l') => {
                if let Some(v) = self.selected_video() {
                    return Action::Play(v.clone());
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
                    return Action::Play(entry.to_video());
                }
            }
            _ => {}
        }
        Action::None
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
        move_list_selection(&mut self.groups_list_state, self.groups.len(), delta);
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
