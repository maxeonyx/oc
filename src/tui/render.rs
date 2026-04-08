use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, layout::Direction};

use super::format::{ColumnWidths, format_column_row, format_memory};
use super::selection::action_label;
use super::state::DashboardState;
use super::types::{DisplayRow, InputMode};

pub fn render(frame: &mut Frame<'_>, state: &DashboardState) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(4),
            Constraint::Length(3),
        ])
        .split(frame.area());

    frame.render_widget(render_summary(state), areas[0]);
    frame.render_widget(render_sessions(state), areas[1]);
    frame.render_widget(render_action_bar(state), areas[2]);
    frame.render_widget(render_input_bar(state), areas[3]);
}

fn render_summary(state: &DashboardState) -> Paragraph<'static> {
    let summary = &state.snapshot.summary;
    Paragraph::new(Line::from(vec![
        Span::raw(format!("Attached: {}", summary.attached)),
        Span::raw("  "),
        Span::raw(format!("Detached: {}", summary.detached)),
        Span::raw("  "),
        Span::raw(format!("Saved: {}", summary.saved)),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Sessions"))
}

fn render_sessions(state: &DashboardState) -> Paragraph<'static> {
    let widths = column_widths(state);

    let mut lines = Vec::new();

    lines.extend(
        state
            .display_rows
            .iter()
            .enumerate()
            .map(|(index, row)| match row {
                DisplayRow::ColumnHeader => Line::from(Span::styled(
                    format_column_row("ID", "NAME", "STATUS", "MEMORY", "DIRECTORY", &widths),
                    Style::default().add_modifier(Modifier::BOLD | Modifier::DIM),
                )),
                DisplayRow::GroupHeader { title } => Line::from(Span::styled(
                    format!("─ {title} ─"),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                DisplayRow::NewSession => Line::from(Span::styled(
                    format_column_row(
                        "+",
                        "New session",
                        "create",
                        "-",
                        "Start a new named session",
                        &widths,
                    ),
                    selection_style(index == state.selected_index),
                )),
                DisplayRow::Session(row) => Line::from(Span::styled(
                    format_column_row(
                        &row.session_id.to_string(),
                        &row.name,
                        row.status_label(),
                        &row.memory_label(),
                        &row.directory,
                        &widths,
                    ),
                    selection_style(index == state.selected_index),
                )),
                DisplayRow::Totals(totals) => Line::from(Span::styled(
                    format_column_row(
                        &totals.filtered_sessions.to_string(),
                        "total sessions",
                        &totals.filtered_running.to_string(),
                        &format_memory(totals.filtered_memory_bytes),
                        "filtered",
                        &widths,
                    ),
                    Style::default().add_modifier(Modifier::DIM | Modifier::BOLD),
                )),
            }),
    );

    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Dashboard"))
}

fn render_action_bar(state: &DashboardState) -> Paragraph<'static> {
    let actions = state
        .available_actions()
        .into_iter()
        .map(action_label)
        .collect::<Vec<_>>()
        .join("   ");
    let selected = action_label(state.selected_action);

    Paragraph::new(vec![
        Line::from(Span::styled(
            format!("←  {selected:^12}  →"),
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("ENTER runs selected action    {actions}"),
            Style::default().add_modifier(Modifier::BOLD),
        )),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Current Action"),
    )
}

fn render_input_bar(state: &DashboardState) -> Paragraph<'static> {
    let mode = match state.input_mode {
        InputMode::Filter => "filter",
        InputMode::Command => "command",
    };

    let status = state.status_message.clone().unwrap_or_default();
    Paragraph::new(vec![
        Line::from(Span::raw(format!("{mode}> {}", state.input_text))),
        Line::from(Span::raw(status)),
    ])
    .block(Block::default().borders(Borders::ALL).title("Input"))
}

fn selection_style(selected: bool) -> Style {
    if selected {
        Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn column_widths(state: &DashboardState) -> ColumnWidths {
    let totals = state.totals();
    let mut widths = ColumnWidths {
        id: "ID".len().max(totals.filtered_sessions.to_string().len()),
        name: "New session"
            .len()
            .max("total sessions".len())
            .max("NAME".len()),
        status: "detached"
            .len()
            .max(totals.filtered_running.to_string().len())
            .max("STATUS".len()),
        memory: "523 MiB"
            .len()
            .max(format_memory(totals.filtered_memory_bytes).len())
            .max("MEMORY".len()),
    };

    for row in &state.snapshot.rows {
        widths.id = widths.id.max(row.session_id.to_string().len());
        widths.name = widths.name.max(row.name.len());
        widths.status = widths.status.max(row.status_label().len());
        widths.memory = widths.memory.max(row.memory_label().len());
    }

    widths
}
