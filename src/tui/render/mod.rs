mod layout;
mod model;
mod theme;

use ratatui::widgets::{Clear, Paragraph, Widget};
use ratatui::{Frame, layout::Rect, style::Style, text::Line};

use super::state::DashboardState;
use layout::{compute_layout, inner_panel_rect};
use model::RenderModel;
use theme::THEME;

pub fn render(frame: &mut Frame<'_>, state: &DashboardState) {
    let render_model = RenderModel::from_state(state, &THEME);
    let layout = compute_layout(frame.area(), &render_model);

    frame.render_widget(Clear, layout.outer);
    fill_rect(frame, layout.outer, Style::default().bg(THEME.outer_bg));

    render_panel(frame, layout.input, &render_model.input_lines);
    render_separator(frame, layout.input_separator, true);
    render_panel(
        frame,
        layout.summary,
        std::slice::from_ref(&render_model.summary_line),
    );
    render_separator(frame, layout.summary_separator, true);
    render_panel(
        frame,
        layout.list,
        &render_model
            .session_table
            .visible_lines(layout.list_inner.height as usize),
    );
    render_separator(frame, layout.list_separator, true);
    render_panel(
        frame,
        layout.totals,
        std::slice::from_ref(&render_model.totals_line),
    );
    render_separator(frame, layout.totals_separator, true);
    render_panel(frame, layout.actions, &render_model.action_lines);
    render_separator(frame, layout.actions_separator, true);
    render_panel(
        frame,
        layout.help,
        std::slice::from_ref(&render_model.help_line),
    );

    let cursor = render_model.cursor_position(layout.input_inner);
    frame.set_cursor_position((cursor.x, cursor.y));
}

fn render_panel(frame: &mut Frame<'_>, area: Rect, lines: &[Line<'static>]) {
    fill_rect(frame, area, Style::default().bg(THEME.panel_bg));
    Paragraph::new(lines.to_vec()).render(inner_panel_rect(area), frame.buffer_mut());
}

fn fill_rect(frame: &mut Frame<'_>, area: Rect, style: Style) {
    for y in area.top()..area.bottom() {
        let line = " ".repeat(area.width as usize);
        Paragraph::new(Line::styled(line, style))
            .render(Rect::new(area.x, y, area.width, 1), frame.buffer_mut());
    }
}

fn render_separator(frame: &mut Frame<'_>, area: Rect, top_panel: bool) {
    if area.height == 0 {
        return;
    }

    let ch = if top_panel { '▄' } else { '▀' };
    let fg = if top_panel {
        THEME.panel_bg
    } else {
        THEME.outer_bg
    };
    let bg = if top_panel {
        THEME.outer_bg
    } else {
        THEME.panel_bg
    };
    let line = ch.to_string().repeat(area.width as usize);
    Paragraph::new(Line::styled(line, Style::default().fg(fg).bg(bg)))
        .render(area, frame.buffer_mut());
}
