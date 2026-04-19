use std::env;
use std::time::Duration;

use palette::{FromColor, Hsl, Srgb};
use ratatui::style::Color;
use terminal_colorsaurus::{
    color_palette, ColorPalette, QueryOptions, ThemeMode as DetectedThemeMode,
};

const COLOR_QUERY_TIMEOUT: Duration = Duration::from_millis(400);
const MIN_PANEL_CONTRAST: f32 = 1.18;
const MIN_BUTTON_CONTRAST: f32 = 1.08;
const MIN_SELECTION_CONTRAST: f32 = 1.35;
const MIN_MUTED_CONTRAST: f32 = 2.4;
const MIN_DISABLED_CONTRAST: f32 = 1.8;
const GROUP_HEADER_TARGET_MIN_CONTRAST: f32 = 1.13;
const GROUP_HEADER_TARGET_MAX_CONTRAST: f32 = 1.17;
const GROUP_HEADER_HARD_MAX_CONTRAST: f32 = 1.20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub outer_bg: Color,
    pub container_bg: Color,
    pub panel_bg: Color,
    pub panel_text: Color,
    pub muted_text: Color,
    pub group_header_text: Color,
    pub totals_text: Color,
    pub accent: Color,
    pub success: Color,
    pub danger: Color,
    pub warning: Color,
    pub selection_bg: Color,
    pub selection_text: Color,
    pub button_bg: Color,
    pub button_text: Color,
    pub disabled_button_bg: Color,
    pub disabled_text: Color,
    pub help_text: Color,
    pub action_attach_bg: Color,
    pub action_attach_text: Color,
    pub action_remove_bg: Color,
    pub action_remove_text: Color,
    pub action_caution_bg: Color,
    pub action_caution_text: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThemeMode {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RgbColor {
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PaletteSeed {
    foreground: RgbColor,
    background: RgbColor,
    mode: ThemeMode,
}

pub fn detect_theme() -> Theme {
    let override_mode = theme_mode_from_override();
    let detected_palette = palette_seed_from_terminal();
    let colorfgbg_palette = palette_seed_from_colorfgbg();
    let mode = override_mode
        .or(detected_palette.map(|palette| palette.mode))
        .or(colorfgbg_palette.map(|palette| palette.mode))
        .unwrap_or(ThemeMode::Dark);

    let palette = detected_palette
        .or(colorfgbg_palette)
        .unwrap_or_else(|| default_palette_seed(mode));

    build_theme(mode, palette.background, palette.foreground)
}

fn build_theme(mode: ThemeMode, background: RgbColor, foreground: RgbColor) -> Theme {
    let container_bg = derive_surface(background, mode, 0.08, 0.55, MIN_PANEL_CONTRAST);
    let panel_bg = derive_nested_surface(container_bg, mode, 0.05, 0.75, 1.12);
    let button_bg = derive_nested_surface(panel_bg, mode, 0.035, 0.75, MIN_BUTTON_CONTRAST);
    let disabled_button_bg = mix(panel_bg, button_bg, 0.22);
    let selection_bg = derive_surface(background, mode, 0.18, 0.7, MIN_SELECTION_CONTRAST);
    let muted_text = derive_subdued_text(panel_bg, foreground, MIN_MUTED_CONTRAST, 0.48);
    let group_header_text = derive_group_header_text(panel_bg, foreground);
    let disabled_text = derive_subdued_text(button_bg, foreground, MIN_DISABLED_CONTRAST, 0.28);
    let selection_text = best_text_color(selection_bg, foreground, background);
    let (action_attach_bg, action_attach_text) =
        semantic_action_pair(4, button_bg, foreground, background);
    let (action_remove_bg, action_remove_text) =
        semantic_action_pair(1, button_bg, foreground, background);
    let (action_caution_bg, action_caution_text) =
        semantic_action_pair(3, button_bg, foreground, background);

    Theme {
        outer_bg: Color::Reset,
        container_bg: container_bg.into(),
        panel_bg: panel_bg.into(),
        panel_text: foreground.into(),
        muted_text: muted_text.into(),
        group_header_text: group_header_text.into(),
        totals_text: Color::Indexed(6),
        accent: Color::Indexed(6),
        success: Color::Indexed(2),
        danger: Color::Indexed(1),
        warning: Color::Indexed(3),
        selection_bg: selection_bg.into(),
        selection_text: selection_text.into(),
        button_bg: button_bg.into(),
        button_text: foreground.into(),
        disabled_button_bg: disabled_button_bg.into(),
        disabled_text: disabled_text.into(),
        help_text: muted_text.into(),
        action_attach_bg,
        action_attach_text,
        action_remove_bg,
        action_remove_text,
        action_caution_bg,
        action_caution_text,
    }
}

fn semantic_action_pair(
    index: u8,
    button_bg: RgbColor,
    foreground: RgbColor,
    background: RgbColor,
) -> (Color, Color) {
    let action_bg = mix(button_bg, ansi_index_rgb(index), 0.45);
    (
        action_bg.into(),
        best_text_color(action_bg, foreground, background).into(),
    )
}

fn palette_seed_from_terminal() -> Option<PaletteSeed> {
    let mut options = QueryOptions::default();
    options.timeout = COLOR_QUERY_TIMEOUT;

    let palette = color_palette(options).ok()?;
    let mode = theme_mode_from_palette(&palette);
    let foreground = palette.foreground.clone().into();
    let background = palette.background.clone().into();

    Some(PaletteSeed {
        foreground,
        background,
        mode,
    })
}

fn theme_mode_from_palette(palette: &ColorPalette) -> ThemeMode {
    match palette.theme_mode() {
        DetectedThemeMode::Light => ThemeMode::Light,
        DetectedThemeMode::Dark => ThemeMode::Dark,
    }
}

fn theme_mode_from_override() -> Option<ThemeMode> {
    match env::var("OC_THEME").ok()?.to_ascii_lowercase().as_str() {
        "light" => Some(ThemeMode::Light),
        "dark" => Some(ThemeMode::Dark),
        _ => None,
    }
}

fn palette_seed_from_colorfgbg() -> Option<PaletteSeed> {
    let (foreground_index, background_index) =
        parse_colorfgbg_indices(&env::var("COLORFGBG").ok()?)?;

    Some(PaletteSeed {
        foreground: ansi_index_rgb(foreground_index),
        background: ansi_index_rgb(background_index),
        mode: if background_index <= 6 || background_index == 8 {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
    })
}

fn parse_colorfgbg_indices(value: &str) -> Option<(u8, u8)> {
    let numbers = value
        .split(';')
        .filter_map(|part| part.parse::<u8>().ok())
        .collect::<Vec<_>>();

    match numbers.as_slice() {
        [foreground, background] => Some((*foreground, *background)),
        [.., foreground, background] => Some((*foreground, *background)),
        _ => None,
    }
}

fn default_palette_seed(mode: ThemeMode) -> PaletteSeed {
    match mode {
        ThemeMode::Light => PaletteSeed {
            foreground: RgbColor::new(0x22, 0x22, 0x22),
            background: RgbColor::new(0xf7, 0xf4, 0xf3),
            mode,
        },
        ThemeMode::Dark => PaletteSeed {
            foreground: RgbColor::new(0xe8, 0xe6, 0xe3),
            background: RgbColor::new(0x14, 0x16, 0x1a),
            mode,
        },
    }
}

fn derive_surface(
    base: RgbColor,
    mode: ThemeMode,
    starting_delta: f32,
    saturation_scale: f32,
    min_contrast: f32,
) -> RgbColor {
    let mut delta = starting_delta;
    let mut candidate = shift_lightness(base, mode, delta, saturation_scale);

    while contrast_ratio(candidate, base) < min_contrast && delta < 0.32 {
        delta += 0.02;
        candidate = shift_lightness(base, mode, delta, saturation_scale);
    }

    candidate
}

fn derive_nested_surface(
    base: RgbColor,
    mode: ThemeMode,
    starting_delta: f32,
    saturation_scale: f32,
    min_contrast: f32,
) -> RgbColor {
    let mut delta = starting_delta;
    let mut candidate = shift_lightness(base, mode, delta, saturation_scale);

    while contrast_ratio(candidate, base) < min_contrast && delta < 0.2 {
        delta += 0.015;
        candidate = shift_lightness(base, mode, delta, saturation_scale);
    }

    candidate
}

fn derive_subdued_text(
    background: RgbColor,
    foreground: RgbColor,
    min_contrast: f32,
    mut foreground_weight: f32,
) -> RgbColor {
    let mut candidate = mix(background, foreground, foreground_weight);

    while contrast_ratio(candidate, background) < min_contrast && foreground_weight < 0.92 {
        foreground_weight += 0.08;
        candidate = mix(background, foreground, foreground_weight);
    }

    candidate
}

fn derive_group_header_text(background: RgbColor, foreground: RgbColor) -> RgbColor {
    let mut best_under_cap = None;
    let mut first_above_min = None;
    let mut fallback = mix(background, foreground, 0.0);
    let mut foreground_weight = 0.0;

    while foreground_weight <= 1.0 {
        let candidate = mix(background, foreground, foreground_weight);
        let contrast = contrast_ratio(candidate, background);

        if contrast <= GROUP_HEADER_HARD_MAX_CONTRAST {
            best_under_cap = Some((candidate, contrast));
        }
        if contrast >= GROUP_HEADER_TARGET_MIN_CONTRAST {
            first_above_min = Some((candidate, contrast));
            break;
        }

        fallback = candidate;
        foreground_weight += 0.01;
    }

    if let Some((candidate, contrast)) = first_above_min {
        if contrast <= GROUP_HEADER_TARGET_MAX_CONTRAST {
            return candidate;
        }
    }

    if let Some((candidate, contrast)) = best_under_cap {
        if contrast >= GROUP_HEADER_TARGET_MIN_CONTRAST {
            return candidate;
        }
    }

    fallback
}

fn best_text_color(
    background: RgbColor,
    terminal_foreground: RgbColor,
    terminal_background: RgbColor,
) -> RgbColor {
    [
        terminal_foreground,
        terminal_background,
        RgbColor::new(0x00, 0x00, 0x00),
        RgbColor::new(0xff, 0xff, 0xff),
    ]
    .into_iter()
    .max_by(|left, right| {
        contrast_ratio(*left, background).total_cmp(&contrast_ratio(*right, background))
    })
    .unwrap_or(terminal_foreground)
}

fn shift_lightness(base: RgbColor, mode: ThemeMode, delta: f32, saturation_scale: f32) -> RgbColor {
    let mut hsl = Hsl::from_color(base.to_srgb());
    hsl.saturation = (hsl.saturation * saturation_scale).clamp(0.0, 1.0);
    hsl.lightness = match mode {
        ThemeMode::Light => (hsl.lightness - delta).clamp(0.0, 1.0),
        ThemeMode::Dark => (hsl.lightness + delta).clamp(0.0, 1.0),
    };

    RgbColor::from_srgb(Srgb::from_color(hsl))
}

fn mix(background: RgbColor, foreground: RgbColor, foreground_weight: f32) -> RgbColor {
    let background_weight = 1.0 - foreground_weight;

    RgbColor::new(
        ((background.r as f32 * background_weight) + (foreground.r as f32 * foreground_weight))
            .round()
            .clamp(0.0, 255.0) as u8,
        ((background.g as f32 * background_weight) + (foreground.g as f32 * foreground_weight))
            .round()
            .clamp(0.0, 255.0) as u8,
        ((background.b as f32 * background_weight) + (foreground.b as f32 * foreground_weight))
            .round()
            .clamp(0.0, 255.0) as u8,
    )
}

fn contrast_ratio(left: RgbColor, right: RgbColor) -> f32 {
    let left_luminance = relative_luminance(left);
    let right_luminance = relative_luminance(right);
    let brighter = left_luminance.max(right_luminance);
    let darker = left_luminance.min(right_luminance);

    (brighter + 0.05) / (darker + 0.05)
}

fn relative_luminance(color: RgbColor) -> f32 {
    let linearize = |component: u8| {
        let channel = component as f32 / 255.0;
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };

    let red = linearize(color.r);
    let green = linearize(color.g);
    let blue = linearize(color.b);

    (0.2126 * red) + (0.7152 * green) + (0.0722 * blue)
}

fn ansi_index_rgb(index: u8) -> RgbColor {
    match index {
        0 => RgbColor::new(0x00, 0x00, 0x00),
        1 => RgbColor::new(0xcd, 0x00, 0x00),
        2 => RgbColor::new(0x00, 0xcd, 0x00),
        3 => RgbColor::new(0xcd, 0xcd, 0x00),
        4 => RgbColor::new(0x00, 0x00, 0xee),
        5 => RgbColor::new(0xcd, 0x00, 0xcd),
        6 => RgbColor::new(0x00, 0xcd, 0xcd),
        7 => RgbColor::new(0xe5, 0xe5, 0xe5),
        8 => RgbColor::new(0x7f, 0x7f, 0x7f),
        9 => RgbColor::new(0xff, 0x00, 0x00),
        10 => RgbColor::new(0x00, 0xff, 0x00),
        11 => RgbColor::new(0xff, 0xff, 0x00),
        12 => RgbColor::new(0x5c, 0x5c, 0xff),
        13 => RgbColor::new(0xff, 0x00, 0xff),
        14 => RgbColor::new(0x00, 0xff, 0xff),
        _ => RgbColor::new(0xff, 0xff, 0xff),
    }
}

impl RgbColor {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    fn to_srgb(self) -> Srgb<f32> {
        Srgb::new(
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        )
    }

    fn from_srgb(color: Srgb<f32>) -> Self {
        Self::new(
            (color.red.clamp(0.0, 1.0) * 255.0).round() as u8,
            (color.green.clamp(0.0, 1.0) * 255.0).round() as u8,
            (color.blue.clamp(0.0, 1.0) * 255.0).round() as u8,
        )
    }
}

impl From<RgbColor> for Color {
    fn from(value: RgbColor) -> Self {
        Color::Rgb(value.r, value.g, value.b)
    }
}

impl From<terminal_colorsaurus::Color> for RgbColor {
    fn from(value: terminal_colorsaurus::Color) -> Self {
        let (r, g, b) = value.scale_to_8bit();
        Self::new(r, g, b)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_theme, contrast_ratio, RgbColor, ThemeMode, GROUP_HEADER_HARD_MAX_CONTRAST,
        GROUP_HEADER_TARGET_MIN_CONTRAST,
    };
    use ratatui::style::Color;

    #[test]
    fn group_header_contrast_stays_within_bound_for_representative_palettes() {
        let representative_palettes = [
            (
                ThemeMode::Light,
                RgbColor::new(0xf7, 0xf4, 0xf3),
                RgbColor::new(0x22, 0x22, 0x22),
            ),
            (
                ThemeMode::Light,
                RgbColor::new(0xff, 0xff, 0xff),
                RgbColor::new(0x00, 0x00, 0x00),
            ),
            (
                ThemeMode::Light,
                RgbColor::new(0xfa, 0xf0, 0xeb),
                RgbColor::new(0x14, 0x28, 0x46),
            ),
            (
                ThemeMode::Dark,
                RgbColor::new(0x14, 0x16, 0x1a),
                RgbColor::new(0xe8, 0xe6, 0xe3),
            ),
            (
                ThemeMode::Dark,
                RgbColor::new(0x00, 0x00, 0x00),
                RgbColor::new(0xff, 0xff, 0xff),
            ),
            (
                ThemeMode::Dark,
                RgbColor::new(0x0a, 0x1e, 0x32),
                RgbColor::new(0xdc, 0xff, 0xdc),
            ),
        ];

        for (mode, background, foreground) in representative_palettes {
            let theme = build_theme(mode, background, foreground);
            let contrast = contrast_ratio(
                color_to_rgb(theme.group_header_text),
                color_to_rgb(theme.panel_bg),
            );

            assert!(
                contrast >= GROUP_HEADER_TARGET_MIN_CONTRAST,
                "group header contrast {contrast:.3} fell below target minimum for mode {mode:?}, background {background:?}, foreground {foreground:?}"
            );
            assert!(
                contrast <= GROUP_HEADER_HARD_MAX_CONTRAST,
                "group header contrast {contrast:.3} exceeded hard maximum for mode {mode:?}, background {background:?}, foreground {foreground:?}"
            );
        }
    }

    fn color_to_rgb(color: Color) -> RgbColor {
        match color {
            Color::Rgb(r, g, b) => RgbColor::new(r, g, b),
            other => panic!("expected rgb color, got {other:?}"),
        }
    }
}
