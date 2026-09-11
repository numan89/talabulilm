use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{App, View};
use crate::groups::GroupKind;
use crate::ytdlp::Video;

const ACCENT: Color = Color::Green;

pub fn draw(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(3), Constraint::Min(3), Constraint::Length(1)])
        .split(f.area());

    draw_title(f, root[0]);
    draw_search_row(f, app, root[1]);

    match app.view {
        View::Search => draw_main_placeholder(f, app, root[2]),
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
        Span::raw("  watch Ceramah Ustaz from the terminal"),
    ]);
    f.render_widget(Paragraph::new(line), cols[0]);

    let credit = Paragraph::new("by Nu'man").style(Style::default().fg(Color::DarkGray)).alignment(Alignment::Right);
    f.render_widget(credit, cols[1]);
}

fn draw_search_row(f: &mut Frame, app: &App, area: Rect) {
    let active = app.view == View::Search;
    let (before, at_cursor, after) = app.input.render_parts();
    let cursor_style = if active { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
    let at_cursor = if at_cursor.is_empty() { " ".to_string() } else { at_cursor };
    let line = Line::from(vec![Span::raw(before), Span::styled(at_cursor, cursor_style), Span::raw(after)]);

    let border_style =
        if active { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) };

    let p = Paragraph::new(line).block(Block::default().borders(Borders::ALL).border_style(border_style).title(" Search (Enter to run) "));
    f.render_widget(p, area);
}

/// Placeholder shown under the Search view before anything has loaded:
/// idle prompt, a spinner-ish "Loading..." while a search is in flight,
/// or an error.
fn draw_main_placeholder(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Results ");
    if app.loading {
        f.render_widget(Paragraph::new("Loading...").block(block), area);
        return;
    }
    if let Some(err) = &app.error {
        f.render_widget(Paragraph::new(err.as_str()).style(Style::default().fg(Color::Red)).block(block), area);
        return;
    }
    let msg = if app.status.is_some() {
        "Nothing found."
    } else {
        "Type a name or topic and press Enter.  Ctrl+U: browse ustaz  Ctrl+H: history"
    };
    f.render_widget(Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)).block(block), area);
}

fn draw_ustaz_list(f: &mut Frame, app: &mut App, area: Rect) {
    let (before, at_cursor, after) = app.ustaz_filter.render_parts();
    let at_cursor = if at_cursor.is_empty() { " ".to_string() } else { at_cursor };
    let filter_line =
        Line::from(vec![Span::raw(before), Span::styled(at_cursor, Style::default().add_modifier(Modifier::REVERSED)), Span::raw(after)]);

    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(1)]).split(area);

    let filter_block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(ACCENT)).title(" Filter ustaz ");
    f.render_widget(Paragraph::new(filter_line).block(filter_block), rows[0]);

    let block = Block::default().borders(Borders::ALL).title(" Ustaz ");
    if app.ustaz_filtered.is_empty() {
        f.render_widget(Paragraph::new("No match.").style(Style::default().fg(Color::DarkGray)).block(block), rows[1]);
        return;
    }
    let items: Vec<ListItem> = app.ustaz_filtered.iter().filter_map(|&i| app.ustaz_names.get(i)).map(|name| ListItem::new(name.clone())).collect();
    let list = List::new(items).block(block).highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, rows[1], &mut app.ustaz_list_state);
}

fn draw_history(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" History ");
    if app.history.is_empty() {
        f.render_widget(Paragraph::new("No watch history yet.").style(Style::default().fg(Color::DarkGray)).block(block), area);
        return;
    }
    let items: Vec<ListItem> = app
        .history
        .iter()
        .map(|e| {
            let line = Line::from(vec![
                Span::styled(format!("{:<60}", truncate(&e.title, 60)), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(e.relative_time(), Style::default().fg(Color::Yellow)),
                Span::raw("  "),
                Span::styled(e.channel.clone(), Style::default().fg(ACCENT)),
            ]);
            ListItem::new(line)
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
    let block = Block::default().borders(Borders::ALL).title(" Videos ");

    if app.results.is_empty() {
        f.render_widget(Paragraph::new("Nothing here.").style(Style::default().fg(Color::DarkGray)).block(block), area);
        return;
    }

    let items: Vec<ListItem> = app.results.iter().map(|v| ListItem::new(video_line(v))).collect();
    let list = List::new(items).block(block).highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, area, &mut app.list_state);
}

/// Ranger-style: the group list on the left, a live preview of whatever
/// group is highlighted on the right — no separate "open" step needed to
/// see what's inside.
fn draw_groups(f: &mut Frame, app: &mut App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let progress = if app.channels_loaded < app.channels_total {
        format!(" Groups ({}/{} channels loaded) ", app.channels_loaded, app.channels_total)
    } else {
        " Groups ".to_string()
    };
    let block = Block::default().borders(Borders::ALL).title(progress);

    if app.groups.is_empty() {
        let msg = if app.channels_loaded < app.channels_total { "Loading..." } else { "Nothing found." };
        f.render_widget(Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)).block(block), panes[0]);
        f.render_widget(Block::default().borders(Borders::ALL).title(" preview "), panes[1]);
        return;
    }

    let items: Vec<ListItem> = app
        .groups
        .iter()
        .map(|g| {
            let (badge, badge_style, label) = match &g.kind {
                GroupKind::Playlist { title } => (" PL ", Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD), title.clone()),
                GroupKind::Other => (
                    " OTHER ",
                    Style::default().fg(Color::Black).bg(Color::Magenta).add_modifier(Modifier::BOLD),
                    "Videos with no playlist".to_string(),
                ),
            };
            let line = Line::from(vec![
                Span::styled(g.channel.clone(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(" > ", Style::default().fg(Color::DarkGray)),
                Span::styled(badge, badge_style),
                Span::raw(" "),
                Span::raw(truncate(&label, 40)),
                Span::styled(format!("  ({})", g.videos.len()), Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(block).highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, panes[0], &mut app.groups_list_state);

    let preview_block = Block::default().borders(Borders::ALL).title(" contents ");
    match app.selected_group() {
        Some(group) => {
            let lines: Vec<Line> = group
                .videos
                .iter()
                .enumerate()
                .map(|(i, v)| Line::from(format!("{:>3}. {}", i + 1, truncate(&v.title, 70))))
                .collect();
            f.render_widget(Paragraph::new(lines).block(preview_block), panes[1]);
        }
        None => f.render_widget(Paragraph::new("").block(preview_block), panes[1]),
    }
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let help = match app.view {
        View::Search => "type to search  Enter run  Tab switch pane  Ctrl+C quit",
        View::Groups => "↑/↓ move  Enter open  Esc/Tab back  q quit",
        View::Results => "↑/↓ move  Enter play  d download  [ / ] quality  Esc/Tab back  q quit",
        View::UstazList => "type to filter  ↑/↓ move  Enter search  Esc back",
        View::History => "↑/↓ move  Enter play  Esc back  q quit",
    };
    let quality = format!("  ·  quality: {}", app.quality.label());
    let status = app.status.as_deref().unwrap_or("");
    let line = Line::from(vec![
        Span::styled(help, Style::default().fg(Color::DarkGray)),
        Span::styled(quality, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(status, Style::default().fg(ACCENT)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let cut: String = s.chars().take(n.saturating_sub(3)).collect();
        format!("{cut}...")
    } else {
        s.to_string()
    }
}
