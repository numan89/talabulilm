use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{App, ContentPane, View};
use crate::groups::GroupKind;
use crate::ytdlp::Video;

const ACCENT: Color = Color::Green;

pub fn draw(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(3), Constraint::Min(3), Constraint::Length(2)])
        .split(f.area());

    draw_title(f, root[0]);
    draw_search_row(f, app, root[1]);

    match app.view {
        // Search only ever has *focus* — the content pane underneath
        // keeps showing whatever was last searched (same as cekhalal
        // leaving its Results pane visible while Search has focus)
        // rather than being hidden behind a separate placeholder.
        View::Search => draw_search_focused_content(f, app, root[2]),
        View::Groups => draw_groups(f, app, root[2]),
        View::Results => draw_results(f, app, root[2]),
        View::UstazList => draw_ustaz_list(f, app, root[2]),
        View::History => draw_history(f, app, root[2]),
    }

    draw_status_bar(f, app, root[3]);
}

fn draw_title(f: &mut Frame, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(12)])
        .split(area);

    let line = Line::from(vec![
        Span::styled(" talabulilm ", Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::raw("  watch Ceramah Ustaz"),
    ]);
    f.render_widget(Paragraph::new(line), cols[0]);

    let credit = Paragraph::new("by Nu'man").style(Style::default().fg(Color::DarkGray)).alignment(Alignment::Right);
    f.render_widget(credit, cols[1]);
}

/// Border color/weight for whichever box currently has real keyboard
/// focus — same `focus_style` idea as cekhalal, so a glance at the border
/// tells you where Tab/Alt+1-3 actually sent you, not just what's on
/// screen (a pane can be visible without being focused, e.g. Groups/
/// Results staying up while Search has focus).
fn focus_style(active: bool) -> Style {
    if active { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) }
}

/// Search box plus the three Channels/Playlists/Uploads limit boxes,
/// Alt+1/2/3 reachable from anywhere — same row layout as cekhalal's
/// Search + Mode/State/Category, so the two apps look and behave alike.
fn draw_search_row(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(20), Constraint::Percentage(20), Constraint::Percentage(20)])
        .split(area);

    let search_active = app.limits_focus.is_none() && app.view == View::Search;
    let (before, at_cursor, after) = app.input.render_parts();
    let cursor_style = if search_active { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
    let at_cursor = if at_cursor.is_empty() { " ".to_string() } else { at_cursor };
    let line = Line::from(vec![Span::raw(before), Span::styled(at_cursor, cursor_style), Span::raw(after)]);

    let search_title = if !search_active {
        "Search (press /)"
    } else if app.input.is_empty() {
        "Search (Enter to run, Esc to quit)"
    } else {
        "Search (Enter to run, Esc to clear)"
    };
    let search = Paragraph::new(line).block(Block::default().borders(Borders::ALL).border_style(focus_style(search_active)).title(search_title));
    f.render_widget(search, cols[0]);

    let limit_fields = [("Channels (Alt+1)", app.limits.channels), ("Playlists (Alt+2)", app.limits.playlists), ("Uploads (Alt+3)", app.limits.uploads)];
    for (i, (title, value)) in limit_fields.into_iter().enumerate() {
        let active = app.limits_focus == Some(i);
        let widget = Paragraph::new(value.to_string())
            .block(Block::default().borders(Borders::ALL).border_style(focus_style(active)).title(title));
        f.render_widget(widget, cols[i + 1]);
    }
}

/// Content area while Search has focus: a search in flight or a failed
/// one is a global state, not tied to whichever pane's data is stale, so
/// both are handled here before falling through to the actual content
/// pane (Groups or Results — which itself covers "never searched yet"
/// and "searched, found nothing").
fn draw_search_focused_content(f: &mut Frame, app: &mut App, area: Rect) {
    // Unfocused, same as the content pane it's standing in for below —
    // Search has focus here, not this area, so the border stays dim
    // rather than defaulting to the terminal's raw (unstyled) border
    // color, which would look inconsistent next to every other pane.
    if app.loading {
        let block = Block::default().borders(Borders::ALL).border_style(focus_style(false)).title("Results");
        f.render_widget(Paragraph::new("Loading...").block(block), area);
        return;
    }
    if let Some(err) = &app.error {
        let block = Block::default().borders(Borders::ALL).border_style(focus_style(false)).title("Results");
        f.render_widget(Paragraph::new(err.as_str()).style(Style::default().fg(Color::Red)).block(block), area);
        return;
    }
    match app.content_pane {
        ContentPane::Groups => draw_groups(f, app, area),
        ContentPane::Results => draw_results(f, app, area),
    }
}

fn draw_ustaz_list(f: &mut Frame, app: &mut App, area: Rect) {
    let (before, at_cursor, after) = app.ustaz_filter.render_parts();
    let at_cursor = if at_cursor.is_empty() { " ".to_string() } else { at_cursor };
    let filter_line =
        Line::from(vec![Span::raw(before), Span::styled(at_cursor, Style::default().add_modifier(Modifier::REVERSED)), Span::raw(after)]);

    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(1)]).split(area);

    let filter_block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(ACCENT)).title("Filter");
    f.render_widget(Paragraph::new(filter_line).block(filter_block), rows[0]);

    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(ACCENT)).title("Ustaz");
    if app.ustaz_filtered.is_empty() {
        f.render_widget(Paragraph::new("No match.").block(block), rows[1]);
        return;
    }
    let items: Vec<ListItem> = app.ustaz_filtered.iter().filter_map(|&i| app.ustaz_names.get(i)).map(|name| ListItem::new(name.clone())).collect();
    let list = List::new(items).block(block).highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, rows[1], &mut app.ustaz_list_state);
}

fn draw_history(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(ACCENT)).title("History");
    if app.history.is_empty() {
        f.render_widget(Paragraph::new("No watch history yet.").block(block), area);
        return;
    }
    let items: Vec<ListItem> = app
        .history
        .iter()
        .map(|e| {
            let progress_color = if e.finished { Color::Green } else { Color::Cyan };
            let title_line = Line::from(vec![
                Span::styled(format!("{:<60}", truncate(&e.title, 60)), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(e.relative_time(), Style::default().fg(Color::Yellow)),
                Span::raw("  "),
                Span::styled(e.channel.clone(), Style::default().fg(ACCENT)),
            ]);
            let progress_line = Line::from(Span::styled(format!("  {}", e.progress_label()), Style::default().fg(progress_color)));
            ListItem::new(vec![title_line, progress_line])
        })
        .collect();
    let list = List::new(items).block(block).highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, area, &mut app.history_list_state);
}

fn video_line(v: &Video) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:<60}", truncate(&v.title, 60)), Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(format!("{:>8}", v.duration), Style::default().fg(Color::Yellow)),
        Span::raw("  "),
        Span::styled(v.channel.clone(), Style::default().fg(ACCENT)),
    ])
}

fn draw_results(f: &mut Frame, app: &mut App, area: Rect) {
    let active = app.limits_focus.is_none() && app.view == View::Results;
    // Before a search ever runs, this pane doesn't yet know whether it'll
    // end up as a flat "Videos" list or hand off to the Groups pane
    // instead — a neutral title here avoids a jarring Videos-\u{2192}Groups
    // flip the moment the first search actually lands on Groups.
    let title = if app.searched_once { "Videos" } else { "Results" };
    let block = Block::default().borders(Borders::ALL).border_style(focus_style(active)).title(title);

    if app.results.is_empty() {
        let msg = if !app.searched_once {
            "Type a name or topic and press Enter to search\n\
             \u{2014} matches channels and automatically groups their videos\n\
             into playlists below."
        } else {
            "Nothing found."
        };
        f.render_widget(Paragraph::new(msg).block(block), area);
        return;
    }

    let items: Vec<ListItem> = app.results.iter().map(|v| ListItem::new(video_line(v))).collect();
    let list = List::new(items).block(block).highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, area, &mut app.list_state);
}

/// Ranger-style: the group list on the left, the selected group's videos
/// on the right. Enter on a group moves control straight into Contents
/// (title + duration already shown there) instead of swapping to a
/// separate full-screen Videos view — there's nothing that view would
/// add that Contents doesn't already have.
fn draw_groups(f: &mut Frame, app: &mut App, area: Rect) {
    let limits_free = app.limits_focus.is_none();
    let list_active = limits_free && app.view == View::Groups && !app.contents_focused;
    let contents_active = limits_free && app.view == View::Groups && app.contents_focused;
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // Paged by channel (n/p), same shape as cekhalal's own "total, page
    // X/Y" Results title. The channel name itself goes in a header row
    // inside the pane rather than the title — a title doesn't wrap and
    // shares space with the counters, whereas the pane's full width
    // gives a long channel name much more room before it needs eliding.
    let progress = if app.channels_loaded < app.channels_total {
        format!("Groups ({}/{} channels loaded)", app.channels_loaded, app.channels_total)
    } else if !app.groups.is_empty() {
        format!("Groups ({} total, page {}/{})", app.groups.len(), app.channel_page + 1, app.channel_page_count())
    } else {
        "Groups".to_string()
    };
    let block = Block::default().borders(Borders::ALL).border_style(focus_style(list_active)).title(progress);
    let inner = block.inner(panes[0]);
    f.render_widget(block, panes[0]);

    if app.groups.is_empty() {
        let msg = if app.channels_loaded < app.channels_total { "Loading..." } else { "Nothing found." };
        f.render_widget(Paragraph::new(msg), inner);
        f.render_widget(Block::default().borders(Borders::ALL).border_style(focus_style(contents_active)).title("Contents"), panes[1]);
        return;
    }

    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Min(1)]).split(inner);
    let channel_name = app.current_channel_name().unwrap_or_default();
    let channel_header = Line::from(vec![
        Span::styled(channel_name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" ({})", app.channel_groups.len()), Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM)),
    ]);
    f.render_widget(Paragraph::new(channel_header), rows[0]);

    let items: Vec<ListItem> = app
        .channel_groups
        .iter()
        .filter_map(|&idx| app.groups.get(idx))
        .map(|g| {
            let (badge, badge_style, label) = match &g.kind {
                GroupKind::Playlist { title } => (" PL ", Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD), title.clone()),
                GroupKind::Other => (
                    " OTHER ",
                    Style::default().fg(Color::Black).bg(Color::Magenta).add_modifier(Modifier::BOLD),
                    "Videos with no playlist".to_string(),
                ),
            };
            // Count uses the same muted Cyan+DIM as elsewhere (Contents
            // numbering, cekhalal's own product numbering) rather than
            // DarkGray: this list's own highlight_style sets a DarkGray
            // background, so DarkGray text here would go invisible on
            // the selected row.
            let line = Line::from(vec![
                Span::styled(badge, badge_style),
                Span::raw(" "),
                Span::raw(truncate(&label, 40)),
                Span::styled(format!(" ({})", g.videos.len()), Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM)),
            ]);
            ListItem::new(line)
        })
        .collect();

    // REVERSED instead of a flat DarkGray background: rows here carry
    // their own PL/OTHER badge backgrounds (green/magenta), and a fixed
    // bg color would stomp those, leaving barely-visible black-on-
    // DarkGray text. REVERSED swaps whatever fg/bg a cell already has,
    // so the badge stays legible on the selected row too.
    let list = List::new(items).highlight_style(Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED));
    f.render_stateful_widget(list, rows[1], &mut app.groups_list_state);

    // Same dynamic title as cekhalal's Preview pane: idle hint, the
    // filter box itself while typing, or a "filtered by X" summary once
    // typing stops but the filter's still applied.
    let contents_title = if app.contents_filter_active {
        let (before, at_cursor, after) = app.contents_filter.render_parts();
        let at_cursor = if at_cursor.is_empty() { " ".to_string() } else { at_cursor };
        let mut spans = vec![Span::raw("Filter videos: "), Span::raw(before)];
        spans.push(Span::styled(at_cursor, Style::default().add_modifier(Modifier::REVERSED)));
        spans.push(Span::raw(after));
        Line::from(spans)
    } else if !app.contents_filter.is_empty() {
        Line::raw(format!("Contents \u{2014} filtered by \"{}\" (Esc to clear)", app.contents_filter.as_string()))
    } else {
        Line::raw("Contents (Enter from Groups to focus, / filters videos)")
    };
    let preview_block = Block::default().borders(Borders::ALL).border_style(focus_style(contents_active)).title(contents_title);

    match app.selected_group() {
        Some(group) if !app.contents_filtered.is_empty() => {
            // Title width follows the pane's actual (inner, border-
            // excluded) width rather than a fixed guess — a fixed one
            // wide enough for the full-width Results pane pushed the
            // duration clean off the edge of this narrower half-pane.
            let inner_width = panes[1].width.saturating_sub(2);
            let items: Vec<ListItem> = app
                .contents_filtered
                .iter()
                .enumerate()
                .filter_map(|(row, &idx)| group.videos.get(idx).map(|v| ListItem::new(content_video_line(row, v, inner_width))))
                .collect();
            let list = List::new(items).block(preview_block).highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
            f.render_stateful_widget(list, panes[1], &mut app.contents_list_state);
        }
        Some(_) if !app.contents_filter.is_empty() => {
            f.render_widget(Paragraph::new("No videos match this filter.").block(preview_block), panes[1]);
        }
        _ => f.render_widget(Paragraph::new("").block(preview_block), panes[1]),
    }
}

fn content_video_line(i: usize, v: &Video, pane_width: u16) -> Line<'static> {
    // Cyan, not DarkGray/Gray: this list's own highlight_style sets a
    // DarkGray background, so DarkGray text would go invisible on the
    // selected row, and Gray maps to the same ANSI white most terminal
    // themes already use as the default foreground — indistinguishable
    // from the (unstyled) title text next to it. Cyan is a genuinely
    // different hue from both that and the Yellow duration.
    let prefix_len = 5; // "NNN. "
    let duration_reserved = 2 + 8; // "  " + up to "99:59:59"
    let title_width = (pane_width as usize).saturating_sub(prefix_len + duration_reserved).max(10);
    Line::from(vec![
        Span::styled(format!("{:>3}. ", i + 1), Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM)),
        Span::styled(format!("{:<title_width$}", truncate(&v.title, title_width)), Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(v.duration.clone(), Style::default().fg(Color::Yellow)),
    ])
}

/// Always-on shortcut lines, present in every view so it's never necessary
/// to remember a binding or hunt for it — same rationale as cekhalal's
/// status bar: guidance stays visible no matter where you are, instead of
/// living only in a one-time placeholder that a result or status message
/// can push off screen. Split across two real rows (not one long string)
/// because a single `Paragraph` row doesn't wrap — anything past the
/// terminal's width is silently clipped, which is exactly how earlier
/// hints (e.g. the Esc-to-quit one) ended up invisible on normal-width
/// terminals despite being present in the code.
fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Length(1)]).split(area);

    let view_help = match app.view {
        // Trimmed to the agreed must-have set for the Search box, kept
        // identical in content between talabulilm and cekhalal: type to
        // search, Tab, Esc, Alt+←/→ word, Alt+Backspace, Ctrl+C
        // (below, in global_help). Enter is already covered by the
        // search box's own title.
        // "Tab → Groups" only once there's actually a search to switch
        // to — showing it before that just points at an empty pane.
        View::Search if app.searched_once => {
            "type to search  Tab \u{2192} Groups  Esc clear/quit  Alt+\u{2190}/\u{2192} word  Alt+Backspace del word"
        }
        View::Search => "type to search  Esc clear/quit  Alt+\u{2190}/\u{2192} word  Alt+Backspace del word",
        View::Groups if app.contents_focused && app.contents_filter_active => "type to filter videos  Enter apply  Esc cancel",
        View::Groups if app.contents_focused => {
            "\u{2191}/\u{2193} move  Enter play  d download  [ / ] quality  / filter videos  Esc/h \u{2192} Groups  Tab \u{2192} Search"
        }
        View::Groups => "\u{2191}/\u{2193} move  Enter \u{2192} Contents  n/p page  Tab/Esc back  / search",
        View::Results => "\u{2191}/\u{2193} move  Tab/Esc back  / search",
        View::UstazList => "type to filter  \u{2191}/\u{2193} move  Enter search  Esc back",
        View::History => "\u{2191}/\u{2193} move  Enter play/resume  f finished  x delete  Esc back  / search",
    };
    let global_help = match app.view {
        // Must-have core lives in view_help above; talabulilm-specific
        // extras (limits jump, ustaz/history screens) still follow here,
        // same as they do for every other view.
        View::Search => "Ctrl+U ustaz  Ctrl+H history",
        View::UstazList => "Ctrl+H history",
        View::History => "Ctrl+U ustaz",
        View::Groups => "Ctrl+U ustaz  Ctrl+H history",
        View::Results => "Enter play  d download  Ctrl+U ustaz  Ctrl+H history",
    };
    // While a Channels/Playlists/Uploads box has focus it fully owns the
    // key (see App::handle_key_limits_focus), so the hint line needs to
    // reflect that instead of whatever the underlying view would show —
    // same idea as cekhalal's Mode/State/Category filter status lines.
    let view_help = if app.limits_focus.is_some() { "\u{2190}/\u{2192} change  Enter search  Tab/Esc back" } else { view_help };
    let quality = format!("quality: {}", app.quality.label());
    let status = app.status.as_deref().unwrap_or("");

    f.render_widget(Paragraph::new(view_help).style(Style::default().fg(Color::DarkGray)), rows[0]);

    let mut spans = vec![
        Span::styled(global_help, Style::default().fg(Color::DarkGray)),
        Span::styled(format!("  \u{b7}  {quality}"), Style::default().fg(Color::DarkGray)),
    ];
    if !status.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(status, Style::default().fg(ACCENT)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), rows[1]);
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let cut: String = s.chars().take(n.saturating_sub(3)).collect();
        format!("{cut}...")
    } else {
        s.to_string()
    }
}
