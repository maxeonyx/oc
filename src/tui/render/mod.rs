mod layout;
mod model;
mod theme;

use ratatui::widgets::{Clear, Paragraph, Widget};
use ratatui::{layout::Rect, style::Style, text::Line, Frame};

use super::state::DashboardState;
use layout::{compute_layout, SurfaceLayout};
use model::RenderModel;

pub use model::{expansion_candidate_metrics, horizontal_metrics, HorizontalMetrics};
pub use theme::{detect_theme, Theme};

pub fn render(frame: &mut Frame<'_>, state: &DashboardState) {
    let metrics = state.effective_horizontal_metrics();
    let render_model = RenderModel::from_state(state, metrics);
    let layout = compute_layout(frame.area(), &render_model);

    frame.render_widget(Clear, frame.area());
    fill_rect(
        frame,
        frame.area(),
        Style::default().bg(state.theme.outer_bg),
    );

    render_surface(
        frame,
        layout.container,
        state.theme.container_bg,
        state.theme.outer_bg,
        &[],
    );
    render_surface(
        frame,
        layout.input,
        state.theme.panel_bg,
        state.theme.container_bg,
        &render_model
            .input_rows
            .iter()
            .map(|row| row.render(layout.input.content.width))
            .collect::<Vec<_>>(),
    );
    render_surface(
        frame,
        layout.summary,
        state.theme.panel_bg,
        state.theme.container_bg,
        &[render_model
            .summary_row
            .render(layout.summary.content.width)],
    );
    render_surface(
        frame,
        layout.list,
        state.theme.panel_bg,
        state.theme.container_bg,
        &render_model.session_table.visible_lines(
            layout.list.content.width,
            layout.list.content.height as usize,
        ),
    );
    render_surface(
        frame,
        layout.actions,
        state.theme.panel_bg,
        state.theme.container_bg,
        &render_model
            .action_rows
            .iter()
            .map(|row| row.render(layout.actions.content.width))
            .collect::<Vec<_>>(),
    );
    render_surface(
        frame,
        layout.help,
        state.theme.panel_bg,
        state.theme.container_bg,
        &[render_model.help_row.render(layout.help.content.width)],
    );

    if render_model.show_cursor() {
        let cursor = render_model.cursor_position(layout.input.content);
        frame.set_cursor_position((cursor.x, cursor.y));
    }
}

fn render_surface(
    frame: &mut Frame<'_>,
    surface: SurfaceLayout,
    bg: ratatui::style::Color,
    parent_bg: ratatui::style::Color,
    lines: &[Line<'static>],
) {
    fill_rect(frame, surface.interior, Style::default().bg(bg));
    render_edge(frame, surface.top_edge, '▄', bg, parent_bg);
    render_edge(frame, surface.bottom_edge, '▀', bg, parent_bg);
    render_lines(frame, surface.content, lines, Style::default().bg(bg));
}

fn render_lines(frame: &mut Frame<'_>, area: Rect, lines: &[Line<'static>], fill_style: Style) {
    let line_count = area.height as usize;
    for index in 0..line_count {
        let row_area = Rect::new(area.x, area.y + index as u16, area.width, 1);
        if let Some(line) = lines.get(index) {
            Paragraph::new(line.clone()).render(row_area, frame.buffer_mut());
        } else {
            Paragraph::new(Line::styled(" ".repeat(area.width as usize), fill_style))
                .render(row_area, frame.buffer_mut());
        }
    }
}

fn fill_rect(frame: &mut Frame<'_>, area: Rect, style: Style) {
    for y in area.top()..area.bottom() {
        Paragraph::new(Line::styled(" ".repeat(area.width as usize), style))
            .render(Rect::new(area.x, y, area.width, 1), frame.buffer_mut());
    }
}

fn render_edge(
    frame: &mut Frame<'_>,
    area: Rect,
    ch: char,
    fg: ratatui::style::Color,
    bg: ratatui::style::Color,
) {
    let line = ch.to_string().repeat(area.width as usize);
    Paragraph::new(Line::styled(line, Style::default().fg(fg).bg(bg)))
        .render(area, frame.buffer_mut());
}
