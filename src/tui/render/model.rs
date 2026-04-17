use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::session::SessionStatus;

use super::theme::Theme;
use crate::tui::format::{centered_rule, format_column_row, format_memory, ColumnWidths};
use crate::tui::state::DashboardState;
use crate::tui::types::{
    ActionState, CursorPosition, DashboardAction, DashboardGroup, DashboardRow, DashboardSnapshot,
    DashboardSummary, InputMode,
};

const MIN_CONTENT_WIDTH: u16 = 40;
const MAX_GROUP_HEADER_LINES: usize = 4;
const SESSION_FOOTER_LINES: usize = 2;
const SESSION_HEADER_LINES: usize = 1;
const BUTTON_MIN_WIDTH: usize = 8;
const BUTTON_SPACING: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HorizontalMetrics {
    pub column_widths: ColumnWidths,
    pub content_width: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DashboardMetrics {
    pub horizontal: HorizontalMetrics,
    pub list_content_height: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActionButtonGeometry {
    width: usize,
    spacing: usize,
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

impl DashboardMetrics {
    pub fn expanded_with(self, other: Self) -> Self {
        Self {
            horizontal: self.horizontal.expanded_with(other.horizontal),
            list_content_height: self.list_content_height.max(other.list_content_height),
        }
    }
}

pub struct RenderModel {
    pub summary_row: RowSpec,
    pub input_rows: Vec<RowSpec>,
    pub session_table: SessionTable,
    action_states: Vec<ActionState>,
    pub help_row: RowSpec,
    input_cursor_offset: u16,
    show_cursor: bool,
}

pub struct SessionTable {
    all_rows: Vec<RowSpec>,
    body_scroll: usize,
}

#[derive(Clone, Debug)]
pub struct RowSpec {
    runs: Vec<StyledRun>,
    fill_style: Style,
}

#[derive(Clone, Debug)]
struct StyledRun {
    text: String,
    style: Style,
}

pub fn horizontal_metrics(state: &DashboardState) -> HorizontalMetrics {
    horizontal_metrics_with_scope(state, MeasurementScope::FrozenLayout)
}

pub fn dashboard_metrics(state: &DashboardState) -> DashboardMetrics {
    DashboardMetrics {
        horizontal: horizontal_metrics(state),
        list_content_height: session_list_content_height(state.view.sessions().count()),
    }
}

pub fn frozen_dashboard_metrics(snapshot: &DashboardSnapshot) -> DashboardMetrics {
    DashboardMetrics {
        horizontal: unfiltered_horizontal_metrics(snapshot),
        list_content_height: frozen_list_content_height(snapshot.rows.len()),
    }
}

pub fn expansion_candidate_metrics(state: &DashboardState) -> HorizontalMetrics {
    horizontal_metrics_with_scope(state, MeasurementScope::ExpansionOnly)
}

impl RenderModel {
    pub fn from_state(state: &DashboardState, metrics: HorizontalMetrics) -> Self {
        let column_widths = metrics.column_widths;
        let session_table = SessionTable::from_state(state, &column_widths, &state.theme);

        Self {
            summary_row: summary_row(state, &state.theme),
            input_rows: input_rows(state, &state.theme),
            session_table,
            action_states: action_states(state),
            help_row: help_row(&state.theme),
            input_cursor_offset: input_cursor_offset(state),
            show_cursor: should_show_cursor(state),
        }
    }

    pub fn input_content_height(&self) -> u16 {
        self.input_rows.len() as u16
    }

    pub fn cursor_position(&self, area: Rect) -> CursorPosition {
        CursorPosition {
            x: area.x + self.input_cursor_offset.min(area.width),
            y: area.y,
        }
    }

    pub fn show_cursor(&self) -> bool {
        self.show_cursor
    }

    pub fn action_rows(&self, theme: &Theme, content_width: u16) -> Vec<RowSpec> {
        action_rows(&self.action_states, theme, content_width)
    }
}

impl SessionTable {
    fn from_state(state: &DashboardState, widths: &ColumnWidths, theme: &Theme) -> Self {
        let all_rows = session_rows(state, widths, theme);
        let footer_start = all_rows.len().saturating_sub(SESSION_FOOTER_LINES);
        let body_line_count = footer_start.saturating_sub(1);
        let selected_body_index = selected_body_line_index(state);

        Self {
            body_scroll: selected_body_index.min(body_line_count.saturating_sub(1)),
            all_rows,
        }
    }

    pub fn visible_lines(&self, width: u16, max_lines: usize) -> Vec<Line<'static>> {
        if self.all_rows.len() <= max_lines {
            return self.all_rows.iter().map(|row| row.render(width)).collect();
        }

        let footer_start = self.all_rows.len().saturating_sub(SESSION_FOOTER_LINES);
        let header = self.all_rows[0].render(width);
        let footer = self.all_rows[footer_start..]
            .iter()
            .map(|row| row.render(width))
            .collect::<Vec<_>>();
        let body = &self.all_rows[1..footer_start];
        let body_space = max_lines.saturating_sub(SESSION_HEADER_LINES + SESSION_FOOTER_LINES);
        let max_scroll = body.len().saturating_sub(body_space);
        let scroll = self.body_scroll.min(max_scroll);
        let visible_body = body
            .iter()
            .skip(scroll)
            .take(body_space)
            .map(|row| row.render(width))
            .collect::<Vec<_>>();

        std::iter::once(header)
            .chain(visible_body)
            .chain(footer)
            .collect()
    }
}

impl RowSpec {
    fn new(fill_style: Style, runs: Vec<StyledRun>) -> Self {
        Self { runs, fill_style }
    }

    fn single(text: impl Into<String>, style: Style) -> Self {
        Self::new(style, vec![StyledRun::new(text, style)])
    }

    pub(crate) fn render(&self, width: u16) -> Line<'static> {
        let target_width = width as usize;
        let mut remaining = target_width;
        let mut spans = Vec::new();

        for run in &self.runs {
            if remaining == 0 {
                break;
            }

            let clipped = clip_text_to_width(&run.text, remaining);
            let clipped_width = display_width(&clipped);
            if clipped_width == 0 {
                continue;
            }

            spans.push(Span::styled(clipped, run.style));
            remaining = remaining.saturating_sub(clipped_width);
        }

        if remaining > 0 {
            spans.push(Span::styled(" ".repeat(remaining), self.fill_style));
        }

        Line::from(spans)
    }
}

impl StyledRun {
    fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

#[derive(Clone, Copy)]
enum MeasurementScope {
    FrozenLayout,
    ExpansionOnly,
}

fn summary_row(state: &DashboardState, theme: &Theme) -> RowSpec {
    let summary = state.summary();
    RowSpec::new(
        Style::default().bg(theme.panel_bg),
        vec![
            StyledRun::new(
                format!("Attached {}", summary.attached),
                Style::default().fg(theme.success).bg(theme.panel_bg),
            ),
            StyledRun::new("  ", Style::default().bg(theme.panel_bg)),
            StyledRun::new(
                format!("Detached {}", summary.detached),
                Style::default().fg(theme.warning).bg(theme.panel_bg),
            ),
            StyledRun::new("  ", Style::default().bg(theme.panel_bg)),
            StyledRun::new(
                format!("Saved {}", summary.saved),
                Style::default().fg(theme.accent).bg(theme.panel_bg),
            ),
        ],
    )
}

fn input_rows(state: &DashboardState, theme: &Theme) -> Vec<RowSpec> {
    let mode = match state.input_mode {
        InputMode::Filter => "filter",
        InputMode::Command => "command",
    };

    let mut rows = vec![RowSpec::new(
        Style::default().bg(theme.panel_bg),
        vec![
            StyledRun::new(
                format!("{mode}> "),
                Style::default().fg(theme.accent).bg(theme.panel_bg),
            ),
            StyledRun::new(
                state.input_text.clone(),
                Style::default().fg(theme.panel_text).bg(theme.panel_bg),
            ),
        ],
    )];

    if let Some(status) = &state.status_message {
        rows.push(RowSpec::single(
            status.clone(),
            Style::default().fg(theme.muted_text).bg(theme.panel_bg),
        ));
    }

    rows
}

fn action_rows(action_states: &[ActionState], theme: &Theme, content_width: u16) -> Vec<RowSpec> {
    let geometry = action_button_geometry(action_states.len(), content_width as usize);

    let mut top = Vec::new();
    let mut middle = Vec::new();
    let mut bottom = Vec::new();

    for (index, action_state) in action_states.iter().enumerate() {
        if index > 0 {
            let spacer = StyledRun::new(
                " ".repeat(geometry.spacing),
                Style::default().bg(theme.panel_bg),
            );
            top.push(spacer.clone());
            middle.push(spacer.clone());
            bottom.push(spacer);
        }

        top.push(action_cap_run(action_state, theme, '▄', geometry.width));
        middle.push(action_label_run(action_state, theme, geometry.width));
        bottom.push(action_cap_run(action_state, theme, '▀', geometry.width));
    }

    vec![
        RowSpec::new(Style::default().bg(theme.panel_bg), top),
        RowSpec::new(Style::default().bg(theme.panel_bg), middle),
        RowSpec::new(Style::default().bg(theme.panel_bg), bottom),
    ]
}

fn help_row(theme: &Theme) -> RowSpec {
    RowSpec::new(
        Style::default().bg(theme.panel_bg),
        vec![
            help_key("↑↓ ", theme),
            help_text("select  ", theme),
            help_key("←→ ", theme),
            help_text("action  ", theme),
            help_key("Enter ", theme),
            help_text("run  ", theme),
            help_key("Space ", theme),
            help_text("command  ", theme),
            help_key("Esc ", theme),
            help_text("clear/quit  ", theme),
            help_key("Ctrl-D ", theme),
            help_text("quit", theme),
        ],
    )
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

fn action_label_run(action_state: &ActionState, theme: &Theme, width: usize) -> StyledRun {
    let text = centered_text(action_state.action.label(), width);
    let style = match (action_state.selected, action_state.enabled) {
        (true, true) => {
            let (fg, bg) = selected_action_colors(action_state.action, theme);
            Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
        }
        (false, true) => Style::default().fg(theme.button_text).bg(theme.button_bg),
        _ => Style::default()
            .fg(theme.disabled_text)
            .bg(theme.disabled_button_bg)
            .add_modifier(Modifier::DIM),
    };

    StyledRun::new(text, style)
}

fn action_cap_run(action_state: &ActionState, theme: &Theme, ch: char, width: usize) -> StyledRun {
    let style = match (action_state.selected, action_state.enabled) {
        (true, true) => {
            let (_fg, bg) = selected_action_colors(action_state.action, theme);
            Style::default().fg(bg).bg(theme.panel_bg)
        }
        (false, true) => Style::default().fg(theme.button_bg).bg(theme.panel_bg),
        _ => Style::default()
            .fg(theme.disabled_button_bg)
            .bg(theme.panel_bg),
    };

    StyledRun::new(ch.to_string().repeat(width), style)
}

fn selected_action_colors(
    action: DashboardAction,
    theme: &Theme,
) -> (ratatui::style::Color, ratatui::style::Color) {
    match action {
        DashboardAction::Attach => (theme.action_attach_text, theme.action_attach_bg),
        DashboardAction::Remove => (theme.action_remove_text, theme.action_remove_bg),
        DashboardAction::Stop | DashboardAction::Restart => {
            (theme.action_caution_text, theme.action_caution_bg)
        }
    }
}

fn help_key(text: &str, theme: &Theme) -> StyledRun {
    StyledRun::new(text, Style::default().fg(theme.accent).bg(theme.panel_bg))
}

fn help_text(text: &str, theme: &Theme) -> StyledRun {
    StyledRun::new(
        text,
        Style::default().fg(theme.help_text).bg(theme.panel_bg),
    )
}

fn horizontal_metrics_with_scope(
    state: &DashboardState,
    scope: MeasurementScope,
) -> HorizontalMetrics {
    let widths = column_widths(state);
    let content_width = content_width_for_scope(state, &widths, scope).max(MIN_CONTENT_WIDTH);

    HorizontalMetrics {
        column_widths: widths,
        content_width,
    }
}

fn unfiltered_horizontal_metrics(snapshot: &DashboardSnapshot) -> HorizontalMetrics {
    let totals = summary_totals(snapshot);
    let widths = column_widths_for_rows(snapshot.rows.iter(), &totals);
    let content_width = unfiltered_content_width(snapshot, &widths).max(MIN_CONTENT_WIDTH);

    HorizontalMetrics {
        column_widths: widths,
        content_width,
    }
}

fn content_width_for_scope(
    state: &DashboardState,
    widths: &ColumnWidths,
    scope: MeasurementScope,
) -> u16 {
    let summary_width = display_width(&format!(
        "Attached {}  Detached {}  Saved {}",
        state.summary().attached,
        state.summary().detached,
        state.summary().saved
    )) as u16;
    let session_width = session_content_width(state, widths);
    let actions_width = actions_content_width();
    let help_width = display_width(
        "↑↓ select  ←→ action  Enter run  Space command  Esc clear/quit  Ctrl-D quit",
    ) as u16;

    let base_width = [summary_width, session_width, actions_width, help_width]
        .into_iter()
        .max()
        .unwrap_or(MIN_CONTENT_WIDTH);

    match scope {
        MeasurementScope::FrozenLayout => base_width,
        MeasurementScope::ExpansionOnly => base_width,
    }
}

fn unfiltered_content_width(snapshot: &DashboardSnapshot, widths: &ColumnWidths) -> u16 {
    let summary_width = display_width(&format!(
        "Attached {}  Detached {}  Saved {}",
        snapshot.summary.attached, snapshot.summary.detached, snapshot.summary.saved
    )) as u16;
    let session_width = unfiltered_session_content_width(snapshot, widths);
    let actions_width = actions_content_width();
    let help_width = display_width(
        "↑↓ select  ←→ action  Enter run  Space command  Esc clear/quit  Ctrl-D quit",
    ) as u16;

    [summary_width, session_width, actions_width, help_width]
        .into_iter()
        .max()
        .unwrap_or(MIN_CONTENT_WIDTH)
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
                display_width(&centered_rule(title, header_width as usize, '─')) as u16
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

fn actions_content_width() -> u16 {
    let count = DashboardAction::DISPLAY_ORDER.len();
    let min_width = DashboardAction::DISPLAY_ORDER
        .iter()
        .map(|action| display_width(action.label()))
        .max()
        .unwrap_or(BUTTON_MIN_WIDTH)
        .max(BUTTON_MIN_WIDTH);
    let geometry = action_button_geometry(
        count,
        (min_width * count) + (BUTTON_SPACING * count.saturating_sub(1)),
    );
    (geometry.width * count + geometry.spacing * count.saturating_sub(1)) as u16
}

fn column_widths(state: &DashboardState) -> ColumnWidths {
    column_widths_for_rows(state.view.sessions(), &state.view.totals)
}

fn column_widths_for_rows<'a>(
    rows: impl IntoIterator<Item = &'a DashboardRow>,
    totals: &DashboardSummary,
) -> ColumnWidths {
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

    for row in rows {
        widths.id = widths.id.max(display_width(&row.session_id.to_string()));
        widths.name = widths.name.max(display_width(&row.name));
        widths.status = widths.status.max(display_width(row.status_label()));
        widths.memory = widths.memory.max(display_width(&row.memory_label()));
    }

    widths
}

fn summary_totals(snapshot: &DashboardSnapshot) -> DashboardSummary {
    DashboardSummary {
        attached: snapshot.summary.attached,
        detached: snapshot.summary.detached,
        saved: snapshot.summary.saved,
        filtered_sessions: snapshot.rows.len(),
        filtered_running: snapshot.summary.attached + snapshot.summary.detached,
        filtered_memory_bytes: snapshot
            .rows
            .iter()
            .map(|row| row.memory_bytes.unwrap_or(0))
            .sum(),
    }
}

fn unfiltered_session_content_width(snapshot: &DashboardSnapshot, widths: &ColumnWidths) -> u16 {
    let totals = summary_totals(snapshot);
    let header_width = display_width(&format_column_row(
        "ID",
        "NAME",
        "STATUS",
        "MEMORY",
        "DIRECTORY",
        widths,
    )) as u16;
    let body_width = snapshot
        .rows
        .iter()
        .map(|row| {
            display_width(&format_column_row(
                &row.session_id.to_string(),
                &row.name,
                row.status_label(),
                &row.memory_label(),
                &row.directory,
                widths,
            )) as u16
        })
        .max()
        .unwrap_or(header_width);
    let totals_width = display_width(&format_column_row(
        &totals.filtered_sessions.to_string(),
        "total sessions",
        &totals.filtered_running.to_string(),
        &format_memory(totals.filtered_memory_bytes),
        "all sessions",
        widths,
    )) as u16;

    header_width.max(body_width).max(totals_width)
}

fn session_list_content_height(session_count: usize) -> u16 {
    (SESSION_HEADER_LINES + session_count + SESSION_FOOTER_LINES) as u16
}

fn frozen_list_content_height(session_count: usize) -> u16 {
    session_list_content_height(session_count).saturating_add(MAX_GROUP_HEADER_LINES as u16)
}

fn session_rows(state: &DashboardState, widths: &ColumnWidths, theme: &Theme) -> Vec<RowSpec> {
    let header_text = format_column_row("ID", "NAME", "STATUS", "MEMORY", "DIRECTORY", widths);
    let header_width = display_width(&header_text);
    let mut rows = vec![RowSpec::single(
        header_text,
        Style::default()
            .fg(theme.accent)
            .bg(theme.panel_bg)
            .add_modifier(Modifier::BOLD),
    )];

    let mut session_index = 0;
    for group in &state.view.groups {
        append_group_rows(
            &mut rows,
            group,
            widths,
            state.selected_index,
            &mut session_index,
            header_width,
            theme,
        );
    }

    rows.push(RowSpec::single("", Style::default().bg(theme.panel_bg)));
    rows.push(RowSpec::single(
        format_column_row(
            &state.view.totals.filtered_sessions.to_string(),
            "total sessions",
            &state.view.totals.filtered_running.to_string(),
            &format_memory(state.view.totals.filtered_memory_bytes),
            state.totals_scope_label(),
            widths,
        ),
        Style::default()
            .fg(theme.totals_text)
            .bg(theme.panel_bg)
            .add_modifier(Modifier::BOLD),
    ));
    rows
}

fn append_group_rows(
    rows: &mut Vec<RowSpec>,
    group: &DashboardGroup,
    widths: &ColumnWidths,
    selected_index: usize,
    session_index: &mut usize,
    header_width: usize,
    theme: &Theme,
) {
    if let Some(title) = &group.title {
        rows.push(RowSpec::single(
            centered_rule(title, header_width, '─'),
            Style::default()
                .fg(theme.group_header_text)
                .bg(theme.panel_bg)
                .add_modifier(Modifier::DIM),
        ));
    }

    for row in &group.sessions {
        rows.push(RowSpec::single(
            format_column_row(
                &row.session_id.to_string(),
                &row.name,
                row.status_label(),
                &row.memory_label(),
                &row.directory,
                widths,
            ),
            row_style(*session_index == selected_index, row, theme),
        ));
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
        Style::default()
            .fg(status_color(row.status, theme))
            .bg(theme.panel_bg)
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

fn should_show_cursor(state: &DashboardState) -> bool {
    matches!(state.input_mode, InputMode::Command)
        || (matches!(state.input_mode, InputMode::Filter) && !state.input_text.is_empty())
}

fn centered_text(label: &str, width: usize) -> String {
    let text_width = display_width(label);
    let total_padding = width.saturating_sub(text_width);
    let left_padding = total_padding / 2;
    let right_padding = total_padding.saturating_sub(left_padding);
    format!(
        "{}{}{}",
        " ".repeat(left_padding),
        label,
        " ".repeat(right_padding)
    )
}

fn clip_text_to_width(text: &str, max_width: usize) -> String {
    let mut clipped = String::new();
    let mut used = 0;

    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > max_width {
            break;
        }
        clipped.push(ch);
        used += ch_width;
    }

    clipped
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn action_button_geometry(button_count: usize, available_width: usize) -> ActionButtonGeometry {
    let spacing = BUTTON_SPACING;
    let gutter_width = spacing * button_count.saturating_sub(1);
    let min_total_width = (BUTTON_MIN_WIDTH * button_count) + gutter_width;
    let usable_width = available_width.max(min_total_width);
    let width = usable_width.saturating_sub(gutter_width) / button_count.max(1);

    ActionButtonGeometry { width, spacing }
}
