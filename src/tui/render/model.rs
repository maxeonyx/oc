use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::session::SessionStatus;

use super::theme::Theme;
use crate::tui::format::{
    ColumnWidths, center_to_display_width, display_width, format_column_row, format_memory,
    pad_to_display_width,
};
use crate::tui::state::DashboardState;
use crate::tui::types::{
    ActionState, CursorPosition, DashboardAction, DashboardGroup, DashboardRow, InputMode,
};

const MIN_CONTENT_WIDTH: u16 = 40;
const SESSION_FOOTER_LINES: usize = 2;
const SESSION_HEADER_LINES: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HorizontalMetrics {
    pub column_widths: ColumnWidths,
    pub content_width: u16,
}

impl HorizontalMetrics {
    pub fn expanded_with(self, other: Self) -> Self {
        Self {
            column_widths: ColumnWidths {
                id: self.column_widths.id.max(other.column_widths.id),
                name: self.column_widths.name.max(other.column_widths.name),
                status: self.column_widths.status.max(other.column_widths.status),
                memory: self.column_widths.memory.max(other.column_widths.memory),
            },
            content_width: self.content_width.max(other.content_width),
        }
    }
}

pub struct RenderModel {
    pub summary_line: Line<'static>,
    pub input_lines: Vec<Line<'static>>,
    pub session_table: SessionTable,
    pub action_lines: Vec<Line<'static>>,
    pub help_line: Line<'static>,
    content_width: u16,
    input_cursor_offset: u16,
}

pub struct SessionTable {
    all_lines: Vec<Line<'static>>,
    body_scroll: usize,
}

pub fn horizontal_metrics(state: &DashboardState) -> HorizontalMetrics {
    horizontal_metrics_with_input(state, true)
}

pub fn expansion_candidate_metrics(state: &DashboardState) -> HorizontalMetrics {
    horizontal_metrics_with_input(state, false)
}

impl RenderModel {
    pub fn from_state(state: &DashboardState, metrics: HorizontalMetrics) -> Self {
        let column_widths = metrics.column_widths;
        let session_table = SessionTable::from_state(state, &column_widths, &state.theme);

        Self {
            summary_line: summary_line(state, &state.theme),
            input_lines: input_lines(state, &state.theme),
            session_table,
            action_lines: action_lines(state, &state.theme),
            help_line: help_line(&state.theme),
            content_width: metrics.content_width,
            input_cursor_offset: input_cursor_offset(state),
        }
    }

    pub fn content_width(&self) -> u16 {
        self.content_width
    }

    pub fn input_content_height(&self) -> u16 {
        self.input_lines.len() as u16
    }

    pub fn cursor_position(&self, area: Rect) -> CursorPosition {
        CursorPosition {
            x: area.x + self.input_cursor_offset,
            y: area.y,
        }
    }
}

impl SessionTable {
    fn from_state(state: &DashboardState, widths: &ColumnWidths, theme: &Theme) -> Self {
        let all_lines = session_lines(state, widths, theme);
        let footer_start = all_lines.len().saturating_sub(SESSION_FOOTER_LINES);
        let body_line_count = footer_start.saturating_sub(1);
        let selected_body_index = selected_body_line_index(state);

        Self {
            body_scroll: selected_body_index.min(body_line_count.saturating_sub(1)),
            all_lines,
        }
    }

    pub fn line_count(&self) -> usize {
        self.all_lines.len()
    }

    pub fn visible_lines(&self, max_lines: usize) -> Vec<Line<'static>> {
        if self.all_lines.len() <= max_lines {
            return self.all_lines.clone();
        }

        let footer_start = self.all_lines.len().saturating_sub(SESSION_FOOTER_LINES);
        let header = self.all_lines[0].clone();
        let footer = self.all_lines[footer_start..].to_vec();
        let body = &self.all_lines[1..footer_start];
        let body_space = max_lines.saturating_sub(SESSION_HEADER_LINES + SESSION_FOOTER_LINES);
        let max_scroll = body.len().saturating_sub(body_space);
        let scroll = self.body_scroll.min(max_scroll);
        let visible_body = body
            .iter()
            .skip(scroll)
            .take(body_space)
            .cloned()
            .collect::<Vec<_>>();

        std::iter::once(header)
            .chain(visible_body)
            .chain(footer)
            .collect()
    }
}

fn summary_line(state: &DashboardState, theme: &Theme) -> Line<'static> {
    let summary = state.summary();
    Line::from(vec![
        Span::styled(
            format!("Attached {}", summary.attached),
            Style::default().fg(theme.success),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Detached {}", summary.detached),
            Style::default().fg(theme.warning),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Saved {}", summary.saved),
            Style::default().fg(theme.accent),
        ),
    ])
}

fn input_lines(state: &DashboardState, theme: &Theme) -> Vec<Line<'static>> {
    let mode = match state.input_mode {
        InputMode::Filter => "filter",
        InputMode::Command => "command",
    };

    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{mode}> "), Style::default().fg(theme.accent)),
        Span::styled(
            state.input_text.clone(),
            Style::default().fg(theme.panel_text),
        ),
    ])];

    if let Some(status) = &state.status_message {
        lines.push(Line::from(Span::styled(
            status.clone(),
            Style::default().fg(theme.muted_text),
        )));
    }

    lines
}

fn action_lines(state: &DashboardState, theme: &Theme) -> Vec<Line<'static>> {
    let action_states = action_states(state);
    let mut action_spans = Vec::new();

    for (index, action_state) in action_states.iter().enumerate() {
        if index > 0 {
            action_spans.push(Span::raw("  "));
        }
        action_spans.push(action_label_span(action_state, theme));
    }

    vec![Line::from(action_spans)]
}

fn help_line(theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled("↑↓ ", Style::default().fg(theme.accent)),
        Span::styled("select  ", Style::default().fg(theme.help_text)),
        Span::styled("←→ ", Style::default().fg(theme.accent)),
        Span::styled("action  ", Style::default().fg(theme.help_text)),
        Span::styled("Enter ", Style::default().fg(theme.accent)),
        Span::styled("run  ", Style::default().fg(theme.help_text)),
        Span::styled("Space ", Style::default().fg(theme.accent)),
        Span::styled("command  ", Style::default().fg(theme.help_text)),
        Span::styled("Esc ", Style::default().fg(theme.accent)),
        Span::styled("clear/quit  ", Style::default().fg(theme.help_text)),
        Span::styled("Ctrl-D ", Style::default().fg(theme.accent)),
        Span::styled("quit", Style::default().fg(theme.help_text)),
    ])
}

fn action_states(state: &DashboardState) -> Vec<ActionState> {
    let available = state.available_actions();
    let has_selection = state.selected_row().is_some();

    DashboardAction::DISPLAY_ORDER
        .into_iter()
        .map(|action| ActionState {
            action,
            enabled: available.contains(&action),
            selected: has_selection
                && available.contains(&action)
                && state.selected_action == action,
        })
        .collect()
}

fn action_label_span(action_state: &ActionState, theme: &Theme) -> Span<'static> {
    match (action_state.selected, action_state.enabled) {
        (true, true) => Span::styled(
            format!(" {} ", action_state.action.label()),
            Style::default()
                .fg(theme.selection_text)
                .bg(theme.selection_bg)
                .add_modifier(Modifier::BOLD),
        ),
        (false, true) => Span::styled(
            format!(" {} ", action_state.action.label()),
            Style::default().fg(theme.panel_text).bg(theme.button_bg),
        ),
        _ => Span::styled(
            format!(" {} ", action_state.action.label()),
            Style::default()
                .fg(theme.disabled_text)
                .bg(theme.panel_bg)
                .add_modifier(Modifier::DIM),
        ),
    }
}

fn horizontal_metrics_with_input(state: &DashboardState, include_input: bool) -> HorizontalMetrics {
    let widths = column_widths(state);
    let content_width = stable_content_width(state, &widths, include_input).max(MIN_CONTENT_WIDTH);

    HorizontalMetrics {
        column_widths: widths,
        content_width,
    }
}

fn stable_content_width(state: &DashboardState, widths: &ColumnWidths, include_input: bool) -> u16 {
    let summary_width = display_width(&format!(
        "Attached {}  Detached {}  Saved {}",
        state.summary().attached,
        state.summary().detached,
        state.summary().saved
    )) as u16;

    let session_width = session_content_width(state, widths);
    let actions_width = action_states(state)
        .iter()
        .enumerate()
        .map(|(index, action_state)| {
            let spacer = if index == 0 { 0 } else { 2 };
            spacer + display_width(&format!(" {} ", action_state.action.label())) as u16
        })
        .sum::<u16>();
    let help_width = display_width(
        "↑↓ select  ←→ action  Enter run  Space command  Esc clear/quit  Ctrl-D quit",
    ) as u16;

    let base_width = [summary_width, session_width, actions_width, help_width]
        .into_iter()
        .max()
        .unwrap_or(MIN_CONTENT_WIDTH);
    if !include_input {
        return base_width;
    }

    let input_width = input_lines(state, &state.theme)
        .iter()
        .map(line_width)
        .max()
        .unwrap_or(0);

    base_width.max(input_width)
}

fn session_content_width(state: &DashboardState, widths: &ColumnWidths) -> u16 {
    let header_width = display_width(&format_column_row(
        "ID",
        "NAME",
        "STATUS",
        "MEMORY",
        "DIRECTORY",
        widths,
    )) as u16;
    let body_width = state
        .view
        .groups
        .iter()
        .flat_map(|group| {
            let group_title = group.title.as_ref().map(|title| {
                display_width(&pad_to_display_width(title, header_width as usize)) as u16
            });
            let rows = group.sessions.iter().map(|row| {
                display_width(&format_column_row(
                    &row.session_id.to_string(),
                    &row.name,
                    row.status_label(),
                    &row.memory_label(),
                    &row.directory,
                    widths,
                )) as u16
            });
            group_title.into_iter().chain(rows)
        })
        .max()
        .unwrap_or(header_width);
    let totals_width = display_width(&format_column_row(
        &state.view.totals.filtered_sessions.to_string(),
        "total sessions",
        &state.view.totals.filtered_running.to_string(),
        &format_memory(state.view.totals.filtered_memory_bytes),
        state.totals_scope_label(),
        widths,
    )) as u16;

    header_width.max(body_width).max(totals_width)
}

fn column_widths(state: &DashboardState) -> ColumnWidths {
    let totals = &state.view.totals;
    let mut widths = ColumnWidths {
        id: display_width("ID").max(display_width(&totals.filtered_sessions.to_string())),
        name: display_width("total sessions").max(display_width("NAME")),
        status: display_width("detached")
            .max(display_width(&totals.filtered_running.to_string()))
            .max(display_width("STATUS")),
        memory: display_width("523 MiB")
            .max(display_width(&format_memory(totals.filtered_memory_bytes)))
            .max(display_width("MEMORY")),
    };

    for row in state.view.sessions() {
        widths.id = widths.id.max(display_width(&row.session_id.to_string()));
        widths.name = widths.name.max(display_width(&row.name));
        widths.status = widths.status.max(display_width(row.status_label()));
        widths.memory = widths.memory.max(display_width(&row.memory_label()));
    }

    widths
}

fn session_lines(
    state: &DashboardState,
    widths: &ColumnWidths,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let header_text = format_column_row("ID", "NAME", "STATUS", "MEMORY", "DIRECTORY", widths);
    let header_width = display_width(&header_text);
    let mut lines = vec![Line::from(Span::styled(
        header_text,
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    ))];

    let mut session_index = 0;
    for group in &state.view.groups {
        append_group_lines(
            &mut lines,
            group,
            widths,
            state.selected_index,
            &mut session_index,
            header_width,
            theme,
        );
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        format_column_row(
            &state.view.totals.filtered_sessions.to_string(),
            "total sessions",
            &state.view.totals.filtered_running.to_string(),
            &format_memory(state.view.totals.filtered_memory_bytes),
            state.totals_scope_label(),
            widths,
        ),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines
}

fn append_group_lines(
    lines: &mut Vec<Line<'static>>,
    group: &DashboardGroup,
    widths: &ColumnWidths,
    selected_index: usize,
    session_index: &mut usize,
    header_width: usize,
    theme: &Theme,
) {
    if let Some(title) = &group.title {
        lines.push(Line::from(Span::styled(
            center_to_display_width(title, header_width),
            Style::default()
                .fg(theme.muted_text)
                .add_modifier(Modifier::DIM),
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
            row_style(*session_index == selected_index, row, theme),
        )));
        *session_index += 1;
    }
}

fn row_style(selected: bool, row: &DashboardRow, theme: &Theme) -> Style {
    if selected {
        Style::default()
            .fg(theme.selection_text)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(status_color(row.status, theme))
    }
}

fn status_color(status: SessionStatus, theme: &Theme) -> ratatui::style::Color {
    match status {
        SessionStatus::RunningAttached => theme.success,
        SessionStatus::RunningDetached => theme.warning,
        SessionStatus::Saved => theme.accent,
    }
}

fn selected_body_line_index(state: &DashboardState) -> usize {
    let mut body_index = 0;
    let mut session_index = 0;
    for group in &state.view.groups {
        if group.title.is_some() {
            body_index += 1;
        }
        for _row in &group.sessions {
            if session_index == state.selected_index {
                return body_index;
            }
            session_index += 1;
            body_index += 1;
        }
    }
    0
}

fn input_cursor_offset(state: &DashboardState) -> u16 {
    let prefix = match state.input_mode {
        InputMode::Filter => "filter> ",
        InputMode::Command => "command> ",
    };

    (display_width(prefix) + display_width(&state.input_text)) as u16
}

fn line_width(line: &Line<'_>) -> u16 {
    line.spans
        .iter()
        .map(|span| display_width(&span.content) as u16)
        .sum()
}
