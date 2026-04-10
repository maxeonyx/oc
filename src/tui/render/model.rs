use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::session::SessionStatus;

use super::theme::Theme;
use crate::tui::format::{
    ColumnWidths, display_width, format_column_row, format_memory, pad_to_display_width,
};
use crate::tui::state::DashboardState;
use crate::tui::types::{
    ActionState, CursorPosition, DashboardAction, DashboardGroup, DashboardRow, InputMode,
};

pub struct RenderModel {
    pub summary_line: Line<'static>,
    pub input_lines: Vec<Line<'static>>,
    pub session_table: SessionTable,
    pub totals_line: Line<'static>,
    pub action_lines: Vec<Line<'static>>,
    pub help_line: Line<'static>,
    content_width: u16,
    input_cursor_offset: u16,
}

pub struct SessionTable {
    all_lines: Vec<Line<'static>>,
    selected_line_index: usize,
}

impl RenderModel {
    pub fn from_state(state: &DashboardState, theme: &Theme) -> Self {
        let column_widths = column_widths(state);
        let session_table = SessionTable::from_state(state, &column_widths, theme);
        let summary_line = summary_line(state, theme);
        let input_lines = input_lines(state, theme);
        let totals_line = totals_line(state, &column_widths, theme);
        let action_lines = action_lines(state, theme);
        let help_line = help_line(theme);
        let input_cursor_offset = input_cursor_offset(state);
        let content_width = [
            line_width(&summary_line),
            lines_width(&input_lines),
            session_table.width(),
            line_width(&totals_line),
            lines_width(&action_lines),
            line_width(&help_line),
        ]
        .into_iter()
        .max()
        .unwrap_or(40);

        Self {
            summary_line,
            input_lines,
            session_table,
            totals_line,
            action_lines,
            help_line,
            content_width,
            input_cursor_offset,
        }
    }

    pub fn content_width(&self) -> u16 {
        self.content_width
    }

    pub fn cursor_position(&self, area: Rect) -> CursorPosition {
        CursorPosition {
            x: area.x + self.input_cursor_offset,
            y: area.y,
        }
    }
}

impl SessionTable {
    fn from_state(state: &DashboardState, column_widths: &ColumnWidths, theme: &Theme) -> Self {
        Self {
            all_lines: session_lines(state, column_widths, theme),
            selected_line_index: selected_line_index(state),
        }
    }

    pub fn line_count(&self) -> usize {
        self.all_lines.len()
    }

    pub fn width(&self) -> u16 {
        lines_width(&self.all_lines)
    }

    pub fn visible_lines(&self, max_lines: usize) -> Vec<Line<'static>> {
        if self.all_lines.len() <= max_lines {
            return self.all_lines.clone();
        }

        let scroll = self
            .selected_line_index
            .saturating_sub(max_lines.saturating_sub(2));
        let header = self.all_lines[0].clone();
        let visible_body = self
            .all_lines
            .iter()
            .skip(scroll.max(1))
            .take(max_lines.saturating_sub(1))
            .cloned()
            .collect::<Vec<_>>();

        std::iter::once(header).chain(visible_body).collect()
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

    vec![
        Line::from(vec![
            Span::styled(format!("{mode}> "), Style::default().fg(theme.accent)),
            Span::styled(
                state.input_text.clone(),
                Style::default().fg(theme.panel_text),
            ),
        ]),
        Line::from(Span::styled(
            state.status_message.clone().unwrap_or_default(),
            Style::default().fg(theme.muted_text),
        )),
    ]
}

fn totals_line(state: &DashboardState, widths: &ColumnWidths, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
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
    ))
}

fn action_lines(state: &DashboardState, theme: &Theme) -> Vec<Line<'static>> {
    let action_states = action_states(state);
    let selected_label = state.selected_action.label();
    let mut action_spans = Vec::new();

    for (index, action_state) in action_states.iter().enumerate() {
        if index > 0 {
            action_spans.push(Span::raw("  "));
        }
        action_spans.push(action_label_span(action_state, theme));
    }

    vec![
        Line::from(action_spans),
        Line::from(vec![
            Span::styled("Enter ", Style::default().fg(theme.accent)),
            Span::styled(
                format!("runs {selected_label}"),
                Style::default().fg(theme.muted_text),
            ),
        ]),
    ]
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

    DashboardAction::DISPLAY_ORDER
        .into_iter()
        .map(|action| ActionState {
            action,
            enabled: available.contains(&action),
            selected: state.selected_action == action,
        })
        .collect()
}

fn action_label_span(action_state: &ActionState, theme: &Theme) -> Span<'static> {
    match (action_state.selected, action_state.enabled) {
        (true, true) => Span::styled(
            format!(" {} ", action_state.action.label()),
            Style::default()
                .fg(Color::Black)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        (true, false) => Span::styled(
            format!(" {} ", action_state.action.label()),
            Style::default()
                .fg(theme.muted_text)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
        (false, true) => Span::styled(
            format!(" {} ", action_state.action.label()),
            Style::default().fg(theme.panel_text).bg(Color::Black),
        ),
        (false, false) => Span::styled(
            format!(" {} ", action_state.action.label()),
            Style::default()
                .fg(theme.help_text)
                .bg(theme.panel_bg)
                .add_modifier(Modifier::DIM),
        ),
    }
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
    let mut lines = vec![Line::from(Span::styled(
        header_text.clone(),
        Style::default()
            .fg(theme.accent)
            .bg(theme.panel_bg)
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
            display_width(&header_text),
            theme,
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
    header_width: usize,
    theme: &Theme,
) {
    if let Some(title) = &group.title {
        lines.push(Line::from(Span::styled(
            pad_to_display_width(title, header_width),
            Style::default().fg(theme.muted_text).bg(theme.panel_bg),
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
            .fg(Color::Black)
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(status_color(row.status, theme))
            .bg(theme.panel_bg)
    }
}

fn status_color(status: SessionStatus, theme: &Theme) -> Color {
    match status {
        SessionStatus::RunningAttached => theme.success,
        SessionStatus::RunningDetached => theme.warning,
        SessionStatus::Saved => theme.accent,
    }
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

fn lines_width(lines: &[Line<'_>]) -> u16 {
    lines.iter().map(line_width).max().unwrap_or(0)
}
