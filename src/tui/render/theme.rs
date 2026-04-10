use ratatui::style::Color;

pub struct Theme {
    pub outer_bg: Color,
    pub panel_bg: Color,
    pub panel_text: Color,
    pub muted_text: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub selection_bg: Color,
    pub help_text: Color,
}

pub const THEME: Theme = Theme {
    outer_bg: Color::Reset,
    panel_bg: Color::DarkGray,
    panel_text: Color::Gray,
    muted_text: Color::DarkGray,
    accent: Color::Cyan,
    success: Color::Green,
    warning: Color::Yellow,
    selection_bg: Color::Blue,
    help_text: Color::Gray,
};
