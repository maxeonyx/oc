use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, layout::Direction};

use super::app::DashboardState;
use super::model::{DashboardAction, DisplayRow, InputMode};

pub fn render(frame: &mut Frame<'_>, state: &DashboardState) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
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

    let lines = state
        .display_rows
        .iter()
        .enumerate()
        .map(|(index, row)| match row {
            DisplayRow::GroupHeader { title } => Line::from(Span::styled(
                format!("─ {title} ─"),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            DisplayRow::NewSession => {
                let content = format!(
                    "{:<id_width$}  {:<name_width$}  {:<status_width$}  {}",
                    "+",
                    "New session",
                    "create",
                    "Start a new named session",
                    id_width = widths.id,
                    name_width = widths.name,
                    status_width = widths.status,
                );

                Line::from(Span::styled(
                    content,
                    selection_style(index == state.selected_index),
                ))
            }
            DisplayRow::Session(row) => {
                let content = format!(
                    "{:<id_width$}  {:<name_width$}  {:<status_width$}  {}",
                    row.session_id,
                    row.name,
                    row.status_label(),
                    row.directory,
                    id_width = widths.id,
                    name_width = widths.name,
                    status_width = widths.status,
                );

                Line::from(Span::styled(
                    content,
                    selection_style(index == state.selected_index),
                ))
            }
        })
        .collect::<Vec<_>>();

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
            format!("Action: {selected}"),
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(Span::raw(actions)),
    ])
    .block(Block::default().borders(Borders::ALL).title("Actions"))
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

fn action_label(action: DashboardAction) -> &'static str {
    match action {
        DashboardAction::Attach => "attach",
        DashboardAction::Stop => "stop",
        DashboardAction::Remove => "rm",
        DashboardAction::Restart => "restart",
        DashboardAction::Create => "create",
    }
}

struct ColumnWidths {
    id: usize,
    name: usize,
    status: usize,
}

fn column_widths(state: &DashboardState) -> ColumnWidths {
    let mut widths = ColumnWidths {
        id: 2,
        name: "New session".len(),
        status: "detached".len(),
    };

    for row in &state.snapshot.rows {
        widths.id = widths.id.max(row.session_id.to_string().len());
        widths.name = widths.name.max(row.name.len());
        widths.status = widths.status.max(row.status_label().len());
    }

    widths
}
