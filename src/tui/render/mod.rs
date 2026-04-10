mod layout;
mod model;
mod theme;

use ratatui::widgets::{Clear, Paragraph, Widget};
use ratatui::{Frame, layout::Rect, style::Style, text::Line};

use super::state::DashboardState;
use layout::{PanelLayout, compute_layout};
use model::RenderModel;

pub use model::{HorizontalMetrics, expansion_candidate_metrics, horizontal_metrics};
pub use theme::{Theme, detect_theme};

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

    render_panel(
        frame,
        layout.input,
        &render_model.input_lines,
        state.theme.panel_bg,
        state.theme.outer_bg,
    );
    render_panel(
        frame,
        layout.summary,
        std::slice::from_ref(&render_model.summary_line),
        state.theme.panel_bg,
        state.theme.outer_bg,
    );
    render_panel(
        frame,
        layout.list,
        &render_model
            .session_table
            .visible_lines(layout.list.content.height as usize),
        state.theme.panel_bg,
        state.theme.outer_bg,
    );
    render_panel(
        frame,
        layout.actions,
        &render_model.action_lines,
        state.theme.panel_bg,
        state.theme.outer_bg,
    );
    render_panel(
        frame,
        layout.help,
        std::slice::from_ref(&render_model.help_line),
        state.theme.panel_bg,
        state.theme.outer_bg,
    );

    let cursor = render_model.cursor_position(layout.input.content);
    frame.set_cursor_position((cursor.x, cursor.y));
}

fn render_panel(
    frame: &mut Frame<'_>,
    panel: PanelLayout,
    lines: &[Line<'static>],
    panel_bg: ratatui::style::Color,
    outer_bg: ratatui::style::Color,
) {
    fill_rect(frame, panel.content, Style::default().bg(panel_bg));
    render_edge(frame, panel.top_edge, '▄', panel_bg, outer_bg);
    render_edge(frame, panel.bottom_edge, '▀', panel_bg, outer_bg);
    Paragraph::new(lines.to_vec()).render(panel.content, frame.buffer_mut());
}

fn fill_rect(frame: &mut Frame<'_>, area: Rect, style: Style) {
    for y in area.top()..area.bottom() {
        let line = " ".repeat(area.width as usize);
        Paragraph::new(Line::styled(line, style))
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
