use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, layout::Direction};

use super::app::DashboardState;

pub fn render(frame: &mut Frame<'_>, state: &DashboardState) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(frame.area());

    frame.render_widget(render_summary(state), areas[0]);
    frame.render_widget(render_sessions(state), areas[1]);
    frame.render_widget(render_footer(), areas[2]);
}

fn render_summary(state: &DashboardState) -> Paragraph<'static> {
    let summary = &state.snapshot.summary;
    let line = Line::from(vec![
        Span::raw(format!("Attached: {}", summary.attached)),
        Span::raw("  "),
        Span::raw(format!("Detached: {}", summary.detached)),
        Span::raw("  "),
        Span::raw(format!("Saved: {}", summary.saved)),
    ]);

    Paragraph::new(vec![line]).block(Block::default().borders(Borders::ALL).title("Sessions"))
}

fn render_sessions(state: &DashboardState) -> Paragraph<'static> {
    let widths = column_widths(state);

    let lines = state
        .snapshot
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
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

            let style = if index == state.selected_index {
                Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                Style::default()
            };

            Line::from(Span::styled(content, style))
        })
        .collect::<Vec<_>>();

    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Dashboard"))
}

fn render_footer() -> Paragraph<'static> {
    Paragraph::new("Enter attach  •  Esc quit")
        .block(Block::default().borders(Borders::ALL).title("Actions"))
}

struct ColumnWidths {
    id: usize,
    name: usize,
    status: usize,
}

fn column_widths(state: &DashboardState) -> ColumnWidths {
    let mut widths = ColumnWidths {
        id: 2,
        name: 4,
        status: 8,
    };

    for row in &state.snapshot.rows {
        widths.id = widths.id.max(row.session_id.to_string().len());
        widths.name = widths.name.max(row.name.len());
        widths.status = widths.status.max(row.status_label().len());
    }

    widths
}

#[allow(dead_code)]
fn _centered_rect(_percent_x: u16, _percent_y: u16, area: Rect) -> Rect {
    area
}
