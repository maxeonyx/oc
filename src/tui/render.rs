use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget};
use ratatui::{layout::Direction, Frame};
use unicode_width::UnicodeWidthStr;

use super::format::{format_column_row, format_memory, ColumnWidths};
use super::selection::action_label;
use super::state::DashboardState;
use super::types::{ActionState, CursorPosition, DashboardAction, DashboardGroup, InputMode};

const OUTER_BG: Color = Color::Reset;
const PANEL_BG: Color = Color::DarkGray;
const PANEL_TEXT: Color = Color::Gray;
const MUTED_TEXT: Color = Color::DarkGray;
const ACCENT: Color = Color::Cyan;
const SUCCESS: Color = Color::Green;
const WARNING: Color = Color::Yellow;
const SELECTION_BG: Color = Color::Blue;
const HELP_TEXT: Color = Color::Gray;
const PANEL_HORIZONTAL_PADDING: u16 = 1;
const PANEL_VERTICAL_PADDING: u16 = 1;
const SECTION_INPUT_HEIGHT: u16 = 4;
const SECTION_SUMMARY_HEIGHT: u16 = 1;
const SECTION_TOTALS_HEIGHT: u16 = 1;
const SECTION_ACTIONS_HEIGHT: u16 = 4;
const SECTION_HELP_HEIGHT: u16 = 1;
const SECTION_SEPARATOR_HEIGHT: u16 = 1;

pub fn render(frame: &mut Frame<'_>, state: &DashboardState) {
    let layout = compute_layout(frame.area(), state);

    frame.render_widget(Clear, layout.outer);
    fill_rect(frame, layout.outer, Style::default().bg(OUTER_BG));

    render_panel(frame, layout.input, render_input_bar(state));
    render_separator(frame, layout.input_separator, true);
    render_panel(frame, layout.summary, render_summary(state));
    render_separator(frame, layout.summary_separator, true);
    render_panel(
        frame,
        layout.list,
        render_sessions(state, layout.list_inner.height as usize),
    );
    render_separator(frame, layout.list_separator, true);
    render_panel(frame, layout.totals, render_totals(state));
    render_separator(frame, layout.totals_separator, true);
    render_panel(frame, layout.actions, render_action_bar(state));
    render_separator(frame, layout.actions_separator, true);
    render_panel(frame, layout.help, render_help_line());

    let cursor = input_cursor_position(layout.input_inner, state);
    frame.set_cursor_position((cursor.x, cursor.y));
}

fn compute_layout(area: Rect, state: &DashboardState) -> DashboardLayout {
    let session_lines = session_lines(state).len().max(1) as u16;
    let list_height = session_lines
        .min(area.height.saturating_sub(fixed_section_height()))
        .max(3);
    let min_height = minimum_panel_height().min(area.height.max(1));
    let desired_height = fixed_section_height() + list_height;
    let outer_height = desired_height.min(area.height).max(min_height);

    let content_width = content_width(state) + 2;
    let outer_width = content_width.min(area.width.saturating_sub(2)).max(40);

    let outer = centered_rect(area, outer_width, outer_height);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(SECTION_INPUT_HEIGHT),
            Constraint::Length(SECTION_SEPARATOR_HEIGHT),
            Constraint::Length(SECTION_SUMMARY_HEIGHT),
            Constraint::Length(SECTION_SEPARATOR_HEIGHT),
            Constraint::Length(list_height + 2),
            Constraint::Length(SECTION_SEPARATOR_HEIGHT),
            Constraint::Length(SECTION_TOTALS_HEIGHT),
            Constraint::Length(SECTION_SEPARATOR_HEIGHT),
            Constraint::Length(SECTION_ACTIONS_HEIGHT),
            Constraint::Length(SECTION_SEPARATOR_HEIGHT),
            Constraint::Length(SECTION_HELP_HEIGHT),
        ])
        .split(outer);

    DashboardLayout {
        outer,
        input: sections[0],
        input_inner: inner_panel_rect(sections[0]),
        input_separator: sections[1],
        summary: sections[2],
        summary_separator: sections[3],
        list: sections[4],
        list_inner: inner_panel_rect(sections[4]),
        list_separator: sections[5],
        totals: sections[6],
        totals_separator: sections[7],
        actions: sections[8],
        actions_separator: sections[9],
        help: sections[10],
    }
}

fn render_summary(state: &DashboardState) -> Paragraph<'static> {
    let summary = state.summary();
    Paragraph::new(Line::from(vec![
        Span::styled(
            format!("Attached {}", summary.attached),
            Style::default().fg(SUCCESS),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Detached {}", summary.detached),
            Style::default().fg(WARNING),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Saved {}", summary.saved),
            Style::default().fg(ACCENT),
        ),
    ]))
    .style(Style::default().bg(PANEL_BG).fg(PANEL_TEXT))
}

fn render_sessions(state: &DashboardState, max_lines: usize) -> Paragraph<'static> {
    let lines = visible_session_lines(state, max_lines);

    Paragraph::new(lines)
        .style(Style::default().bg(PANEL_BG).fg(PANEL_TEXT))
        .alignment(Alignment::Left)
}

fn render_totals(state: &DashboardState) -> Paragraph<'static> {
    let widths = column_widths(state);
    Paragraph::new(Line::from(Span::styled(
        format_column_row(
            &state.view.totals.filtered_sessions.to_string(),
            "total sessions",
            &state.view.totals.filtered_running.to_string(),
            &format_memory(state.view.totals.filtered_memory_bytes),
            state.totals_scope_label(),
            &widths,
        ),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )))
    .style(Style::default().bg(PANEL_BG).fg(PANEL_TEXT))
}

fn render_action_bar(state: &DashboardState) -> Paragraph<'static> {
    let action_states = action_states(state);
    let selected = action_label(state.selected_action);
    let mut action_spans = Vec::new();
    for (index, action_state) in action_states.iter().enumerate() {
        if index > 0 {
            action_spans.push(Span::raw("  "));
        }
        action_spans.push(render_action_label(action_state));
    }

    Paragraph::new(vec![
        Line::from(action_spans),
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(ACCENT)),
            Span::styled(format!("runs {selected}"), Style::default().fg(MUTED_TEXT)),
        ]),
    ])
    .style(Style::default().bg(PANEL_BG).fg(PANEL_TEXT))
}

fn render_input_bar(state: &DashboardState) -> Paragraph<'static> {
    let mode = match state.input_mode {
        InputMode::Filter => "filter",
        InputMode::Command => "command",
    };

    let status = state.status_message.clone().unwrap_or_default();
    Paragraph::new(vec![
        Line::from(vec![
            Span::styled(format!("{mode}> "), Style::default().fg(ACCENT)),
            Span::styled(state.input_text.clone(), Style::default().fg(PANEL_TEXT)),
        ]),
        Line::from(Span::styled(status, Style::default().fg(MUTED_TEXT))),
    ])
    .style(Style::default().bg(PANEL_BG).fg(PANEL_TEXT))
}

fn render_help_line() -> Paragraph<'static> {
    Paragraph::new(Line::from(vec![
        Span::styled("↑↓ ", Style::default().fg(ACCENT)),
        Span::styled("select  ", Style::default().fg(HELP_TEXT)),
        Span::styled("←→ ", Style::default().fg(ACCENT)),
        Span::styled("action  ", Style::default().fg(HELP_TEXT)),
        Span::styled("Enter ", Style::default().fg(ACCENT)),
        Span::styled("run  ", Style::default().fg(HELP_TEXT)),
        Span::styled("Space ", Style::default().fg(ACCENT)),
        Span::styled("command  ", Style::default().fg(HELP_TEXT)),
        Span::styled("Esc ", Style::default().fg(ACCENT)),
        Span::styled("clear/quit  ", Style::default().fg(HELP_TEXT)),
        Span::styled("Ctrl-D ", Style::default().fg(ACCENT)),
        Span::styled("quit", Style::default().fg(HELP_TEXT)),
    ]))
    .style(Style::default().bg(PANEL_BG).fg(PANEL_TEXT))
}

fn selection_style(selected: bool, status: &str) -> Style {
    if selected {
        Style::default()
            .fg(Color::Black)
            .bg(SELECTION_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(status_color(status)).bg(PANEL_BG)
    }
}

fn column_widths(state: &DashboardState) -> ColumnWidths {
    let totals = &state.view.totals;
    let mut widths = ColumnWidths {
        id: "ID".len().max(totals.filtered_sessions.to_string().len()),
        name: "total sessions".len().max("NAME".len()),
        status: "detached"
            .len()
            .max(totals.filtered_running.to_string().len())
            .max("STATUS".len()),
        memory: "523 MiB"
            .len()
            .max(format_memory(totals.filtered_memory_bytes).len())
            .max("MEMORY".len()),
    };

    for row in state.view.sessions() {
        widths.id = widths.id.max(row.session_id.to_string().len());
        widths.name = widths.name.max(row.name.len());
        widths.status = widths.status.max(row.status_label().len());
        widths.memory = widths.memory.max(row.memory_label().len());
    }

    widths
}

fn action_states(state: &DashboardState) -> Vec<ActionState> {
    let available = state.available_actions();

    DashboardAction::ALL
        .into_iter()
        .map(|action| ActionState {
            action,
            enabled: available.contains(&action),
            selected: state.selected_action == action,
        })
        .collect()
}

fn render_action_label(action_state: &ActionState) -> Span<'static> {
    match (action_state.selected, action_state.enabled) {
        (true, true) => Span::styled(
            format!(" {} ", action_label(action_state.action)),
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        (true, false) => Span::styled(
            format!(" {} ", action_label(action_state.action)),
            Style::default()
                .fg(MUTED_TEXT)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
        (false, true) => Span::styled(
            format!(" {} ", action_label(action_state.action)),
            Style::default().fg(PANEL_TEXT).bg(Color::Black),
        ),
        (false, false) => Span::styled(
            format!(" {} ", action_label(action_state.action)),
            Style::default()
                .fg(HELP_TEXT)
                .bg(PANEL_BG)
                .add_modifier(Modifier::DIM),
        ),
    }
}

fn render_panel(frame: &mut Frame<'_>, area: Rect, paragraph: Paragraph<'_>) {
    fill_rect(frame, area, Style::default().bg(PANEL_BG));
    paragraph.render(inner_panel_rect(area), frame.buffer_mut());
}

fn fill_rect(frame: &mut Frame<'_>, area: Rect, style: Style) {
    for y in area.top()..area.bottom() {
        let line = " ".repeat(area.width as usize);
        Paragraph::new(Line::from(Span::styled(line, style)))
            .render(Rect::new(area.x, y, area.width, 1), frame.buffer_mut());
    }
}

fn render_separator(frame: &mut Frame<'_>, area: Rect, top_panel: bool) {
    if area.height == 0 {
        return;
    }

    let ch = if top_panel { '▄' } else { '▀' };
    let fg = if top_panel { PANEL_BG } else { OUTER_BG };
    let bg = if top_panel { OUTER_BG } else { PANEL_BG };
    let line = ch.to_string().repeat(area.width as usize);
    Paragraph::new(Line::from(Span::styled(
        line,
        Style::default().fg(fg).bg(bg),
    )))
    .render(area, frame.buffer_mut());
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width),
            Constraint::Fill(1),
        ])
        .split(area);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .split(horizontal[1]);
    vertical[1]
}

fn inner_panel_rect(area: Rect) -> Rect {
    area.inner(Margin {
        horizontal: PANEL_HORIZONTAL_PADDING,
        vertical: if area.height > 2 {
            PANEL_VERTICAL_PADDING
        } else {
            0
        },
    })
}

fn session_lines(state: &DashboardState) -> Vec<Line<'static>> {
    let widths = column_widths(state);
    let mut lines = vec![Line::from(Span::styled(
        format_column_row("ID", "NAME", "STATUS", "MEMORY", "DIRECTORY", &widths),
        Style::default()
            .fg(ACCENT)
            .bg(PANEL_BG)
            .add_modifier(Modifier::BOLD),
    ))];

    let mut session_index = 0;
    for group in &state.view.groups {
        append_group_lines(
            &mut lines,
            group,
            &widths,
            state.selected_index,
            &mut session_index,
        );
    }
    lines
}

fn append_group_lines(
    lines: &mut Vec<Line<'static>>,
    group: &DashboardGroup,
    widths: &ColumnWidths,
    selected_index: usize,
    session_index: &mut usize,
) {
    if let Some(title) = &group.title {
        let header_width =
            format_column_row("ID", "NAME", "STATUS", "MEMORY", "DIRECTORY", widths).len();
        lines.push(Line::from(Span::styled(
            format!("{title:<header_width$}"),
            Style::default().fg(MUTED_TEXT).bg(PANEL_BG),
        )));
    }

    for row in &group.sessions {
        lines.push(Line::from(Span::styled(
            format_column_row(
                &row.session_id.to_string(),
                &row.name,
                row.status_label(),
                &row.memory_label(),
                &row.directory,
                widths,
            ),
            selection_style(*session_index == selected_index, row.status_label()),
        )));
        *session_index += 1;
    }
}

fn visible_session_lines(state: &DashboardState, max_lines: usize) -> Vec<Line<'static>> {
    let all_lines = session_lines(state);
    if all_lines.len() <= max_lines {
        return all_lines;
    }

    let selected_line = selected_line_index(state);
    let scroll = selected_line.saturating_sub(max_lines.saturating_sub(2));
    let header = all_lines[0].clone();
    let visible_body = all_lines
        .into_iter()
        .skip(scroll.max(1))
        .take(max_lines.saturating_sub(1))
        .collect::<Vec<_>>();

    std::iter::once(header).chain(visible_body).collect()
}

fn selected_line_index(state: &DashboardState) -> usize {
    let mut line_index = 1;
    let mut session_index = 0;
    for group in &state.view.groups {
        if group.title.is_some() {
            line_index += 1;
        }
        for _row in &group.sessions {
            if session_index == state.selected_index {
                return line_index;
            }
            session_index += 1;
            line_index += 1;
        }
    }
    1
}

fn status_color(status: &str) -> Color {
    match status {
        "attached" => SUCCESS,
        "detached" => WARNING,
        "saved" => ACCENT,
        _ => PANEL_TEXT,
    }
}

fn content_width(state: &DashboardState) -> u16 {
    let widths = column_widths(state);

    let summary_width = display_width(&format!(
        "Attached {}  Detached {}  Saved {}",
        state.summary().attached,
        state.summary().detached,
        state.summary().saved
    ));

    let input_width = display_width(&format!(
        "{}> {}",
        match state.input_mode {
            InputMode::Filter => "filter",
            InputMode::Command => "command",
        },
        state.input_text
    ));

    let status_width = display_width(state.status_message.as_deref().unwrap_or(""));

    let session_width = session_lines(state)
        .iter()
        .map(line_width)
        .max()
        .unwrap_or(0);

    let totals_width = display_width(&format_column_row(
        &state.view.totals.filtered_sessions.to_string(),
        "total sessions",
        &state.view.totals.filtered_running.to_string(),
        &format_memory(state.view.totals.filtered_memory_bytes),
        state.totals_scope_label(),
        &widths,
    ));

    let actions_width = action_line_width(state);
    let action_hint_width = display_width(&format!(
        "Enter runs {}",
        action_label(state.selected_action)
    ));
    let help_width = help_line_width();

    [
        summary_width,
        input_width,
        status_width,
        session_width,
        totals_width,
        actions_width,
        action_hint_width,
        help_width,
    ]
    .into_iter()
    .max()
    .unwrap_or(40)
}

fn line_width(line: &Line<'_>) -> u16 {
    line.spans
        .iter()
        .map(|span| display_width(&span.content))
        .sum()
}

fn action_line_width(state: &DashboardState) -> u16 {
    let states = action_states(state);
    let mut width = 0u16;
    for (index, action_state) in states.iter().enumerate() {
        if index > 0 {
            width += 2;
        }
        width += match (action_state.selected, action_state.enabled) {
            (true, _) | (false, _) => display_width(action_label(action_state.action)) + 2,
        };
    }
    width
}

fn help_line_width() -> u16 {
    display_width("↑↓ select  ←→ action  Enter run  Space command  Esc clear/quit  Ctrl-D quit")
}

fn display_width(text: &str) -> u16 {
    UnicodeWidthStr::width(text) as u16
}

fn input_cursor_position(area: Rect, state: &DashboardState) -> CursorPosition {
    let mode_len = match state.input_mode {
        InputMode::Filter => "filter> ".len(),
        InputMode::Command => "command> ".len(),
    } as u16;

    CursorPosition {
        x: area.x + mode_len + state.input_text.chars().count() as u16,
        y: area.y,
    }
}

fn fixed_section_height() -> u16 {
    SECTION_INPUT_HEIGHT
        + SECTION_SUMMARY_HEIGHT
        + SECTION_TOTALS_HEIGHT
        + SECTION_ACTIONS_HEIGHT
        + SECTION_HELP_HEIGHT
        + (SECTION_SEPARATOR_HEIGHT * 5)
}

fn minimum_panel_height() -> u16 {
    SECTION_INPUT_HEIGHT
        + SECTION_SEPARATOR_HEIGHT
        + SECTION_SUMMARY_HEIGHT
        + SECTION_SEPARATOR_HEIGHT
        + 3
        + SECTION_SEPARATOR_HEIGHT
        + SECTION_TOTALS_HEIGHT
        + SECTION_SEPARATOR_HEIGHT
        + SECTION_ACTIONS_HEIGHT
        + SECTION_SEPARATOR_HEIGHT
        + SECTION_HELP_HEIGHT
}

#[derive(Clone, Copy)]
struct DashboardLayout {
    outer: Rect,
    input: Rect,
    input_inner: Rect,
    input_separator: Rect,
    summary: Rect,
    summary_separator: Rect,
    list: Rect,
    list_inner: Rect,
    list_separator: Rect,
    totals: Rect,
    totals_separator: Rect,
    actions: Rect,
    actions_separator: Rect,
    help: Rect,
}
