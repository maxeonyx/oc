use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};

use super::model::RenderModel;

const PANEL_HORIZONTAL_PADDING: u16 = 1;
const PANEL_EDGE_HEIGHT: u16 = 1;
const SUMMARY_CONTENT_HEIGHT: u16 = 1;
const ACTIONS_CONTENT_HEIGHT: u16 = 1;
const HELP_CONTENT_HEIGHT: u16 = 1;
const MIN_INPUT_CONTENT_HEIGHT: u16 = 1;
const MAX_INPUT_CONTENT_HEIGHT: u16 = 2;
const MIN_LIST_CONTENT_HEIGHT: u16 = 3;
const MIN_OUTER_WIDTH: u16 = 40;
const OUTER_HORIZONTAL_MARGIN: u16 = 2;

pub fn compute_layout(area: Rect, render_model: &RenderModel) -> DashboardLayout {
    let input_content_height = render_model
        .input_content_height()
        .clamp(MIN_INPUT_CONTENT_HEIGHT, MAX_INPUT_CONTENT_HEIGHT);
    let fixed_height = fixed_section_height(input_content_height);
    let list_content_height = (render_model
        .session_table
        .line_count()
        .max(MIN_LIST_CONTENT_HEIGHT as usize) as u16)
        .min(area.height.saturating_sub(fixed_height))
        .max(MIN_LIST_CONTENT_HEIGHT);
    let min_height = minimum_panel_height(input_content_height).min(area.height.max(1));
    let desired_height = fixed_height + list_content_height;
    let outer_height = desired_height.min(area.height).max(min_height);

    let desired_width = render_model
        .content_width()
        .saturating_add(PANEL_HORIZONTAL_PADDING * 2);
    let max_width = area.width.saturating_sub(OUTER_HORIZONTAL_MARGIN);
    let outer_width = clamp_outer_width(desired_width, max_width);

    let outer = centered_rect(area, outer_width, outer_height);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(panel_height(input_content_height)),
            Constraint::Length(panel_height(SUMMARY_CONTENT_HEIGHT)),
            Constraint::Length(panel_height(list_content_height)),
            Constraint::Length(panel_height(ACTIONS_CONTENT_HEIGHT)),
            Constraint::Length(panel_height(HELP_CONTENT_HEIGHT)),
        ])
        .split(outer);

    DashboardLayout {
        outer,
        input: PanelLayout::new(sections[0]),
        summary: PanelLayout::new(sections[1]),
        list: PanelLayout::new(sections[2]),
        actions: PanelLayout::new(sections[3]),
        help: PanelLayout::new(sections[4]),
    }
}

fn panel_height(content_height: u16) -> u16 {
    content_height + (PANEL_EDGE_HEIGHT * 2)
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

fn fixed_section_height(input_content_height: u16) -> u16 {
    panel_height(input_content_height)
        + panel_height(SUMMARY_CONTENT_HEIGHT)
        + panel_height(ACTIONS_CONTENT_HEIGHT)
        + panel_height(HELP_CONTENT_HEIGHT)
        + (PANEL_EDGE_HEIGHT * 2)
}

fn minimum_panel_height(input_content_height: u16) -> u16 {
    panel_height(input_content_height)
        + panel_height(SUMMARY_CONTENT_HEIGHT)
        + panel_height(MIN_LIST_CONTENT_HEIGHT)
        + panel_height(ACTIONS_CONTENT_HEIGHT)
        + panel_height(HELP_CONTENT_HEIGHT)
}

#[derive(Clone, Copy)]
pub struct PanelLayout {
    pub content: Rect,
    pub top_edge: Rect,
    pub bottom_edge: Rect,
}

impl PanelLayout {
    fn new(area: Rect) -> Self {
        let top_edge = Rect::new(area.x, area.y, area.width, PANEL_EDGE_HEIGHT);
        let bottom_edge = Rect::new(
            area.x,
            area.bottom().saturating_sub(PANEL_EDGE_HEIGHT),
            area.width,
            PANEL_EDGE_HEIGHT,
        );
        let content = area.inner(Margin {
            horizontal: PANEL_HORIZONTAL_PADDING,
            vertical: PANEL_EDGE_HEIGHT,
        });

        Self {
            content,
            top_edge,
            bottom_edge,
        }
    }
}

#[derive(Clone, Copy)]
pub struct DashboardLayout {
    pub outer: Rect,
    pub input: PanelLayout,
    pub summary: PanelLayout,
    pub list: PanelLayout,
    pub actions: PanelLayout,
    pub help: PanelLayout,
}
