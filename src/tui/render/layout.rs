use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};

use super::model::RenderModel;

const PANEL_HORIZONTAL_PADDING: u16 = 1;
const PANEL_VERTICAL_PADDING: u16 = 1;
const SECTION_INPUT_HEIGHT: u16 = 4;
const SECTION_SUMMARY_HEIGHT: u16 = 1;
const SECTION_TOTALS_HEIGHT: u16 = 1;
const SECTION_ACTIONS_HEIGHT: u16 = 4;
const SECTION_HELP_HEIGHT: u16 = 1;
const SECTION_SEPARATOR_HEIGHT: u16 = 1;
const MIN_OUTER_WIDTH: u16 = 40;
const OUTER_HORIZONTAL_MARGIN: u16 = 2;
const LIST_CHROME_HEIGHT: u16 = 2;
const MIN_LIST_HEIGHT: u16 = 3;

pub fn compute_layout(area: Rect, render_model: &RenderModel) -> DashboardLayout {
    let session_lines = render_model.session_table.line_count().max(1) as u16;
    let list_height = session_lines
        .min(area.height.saturating_sub(fixed_section_height()))
        .max(MIN_LIST_HEIGHT);
    let min_height = minimum_panel_height().min(area.height.max(1));
    let desired_height = fixed_section_height() + list_height;
    let outer_height = desired_height.min(area.height).max(min_height);

    let desired_width = render_model.content_width().saturating_add(2);
    let max_width = area.width.saturating_sub(OUTER_HORIZONTAL_MARGIN);
    let outer_width = clamp_outer_width(desired_width, max_width);

    let outer = centered_rect(area, outer_width, outer_height);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(SECTION_INPUT_HEIGHT),
            Constraint::Length(SECTION_SEPARATOR_HEIGHT),
            Constraint::Length(SECTION_SUMMARY_HEIGHT),
            Constraint::Length(SECTION_SEPARATOR_HEIGHT),
            Constraint::Length(list_height + LIST_CHROME_HEIGHT),
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

pub fn inner_panel_rect(area: Rect) -> Rect {
    area.inner(Margin {
        horizontal: PANEL_HORIZONTAL_PADDING,
        vertical: if area.height > 2 {
            PANEL_VERTICAL_PADDING
        } else {
            0
        },
    })
}

fn clamp_outer_width(desired_width: u16, max_width: u16) -> u16 {
    if max_width == 0 {
        return 0;
    }

    if max_width < MIN_OUTER_WIDTH {
        return max_width;
    }

    desired_width.min(max_width).max(MIN_OUTER_WIDTH)
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

fn fixed_section_height() -> u16 {
    SECTION_INPUT_HEIGHT
        + SECTION_SUMMARY_HEIGHT
        + SECTION_TOTALS_HEIGHT
        + SECTION_ACTIONS_HEIGHT
        + SECTION_HELP_HEIGHT
        + LIST_CHROME_HEIGHT
        + (SECTION_SEPARATOR_HEIGHT * 5)
}

fn minimum_panel_height() -> u16 {
    SECTION_INPUT_HEIGHT
        + SECTION_SEPARATOR_HEIGHT
        + SECTION_SUMMARY_HEIGHT
        + SECTION_SEPARATOR_HEIGHT
        + MIN_LIST_HEIGHT
        + LIST_CHROME_HEIGHT
        + SECTION_SEPARATOR_HEIGHT
        + SECTION_TOTALS_HEIGHT
        + SECTION_SEPARATOR_HEIGHT
        + SECTION_ACTIONS_HEIGHT
        + SECTION_SEPARATOR_HEIGHT
        + SECTION_HELP_HEIGHT
}

#[derive(Clone, Copy)]
pub struct DashboardLayout {
    pub outer: Rect,
    pub input: Rect,
    pub input_inner: Rect,
    pub input_separator: Rect,
    pub summary: Rect,
    pub summary_separator: Rect,
    pub list: Rect,
    pub list_inner: Rect,
    pub list_separator: Rect,
    pub totals: Rect,
    pub totals_separator: Rect,
    pub actions: Rect,
    pub actions_separator: Rect,
    pub help: Rect,
}
