use std::env;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

use ratatui::style::Color;

const O_NONBLOCK: i32 = 0o4000;
const OSC_QUERY_TIMEOUT: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub outer_bg: Color,
    pub panel_bg: Color,
    pub panel_text: Color,
    pub muted_text: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub selection_bg: Color,
    pub selection_text: Color,
    pub button_bg: Color,
    pub disabled_text: Color,
    pub help_text: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThemeMode {
    Light,
    Dark,
}

pub fn detect_theme() -> Theme {
    match detect_theme_mode() {
        ThemeMode::Light => light_theme(),
        ThemeMode::Dark => dark_theme(),
    }
}

fn detect_theme_mode() -> ThemeMode {
    if let Some(mode) = theme_mode_from_override() {
        return mode;
    }

    if let Some(mode) = theme_mode_from_osc_11() {
        return mode;
    }

    if let Some(mode) = theme_mode_from_colorfgbg() {
        return mode;
    }

    ThemeMode::Dark
}

fn theme_mode_from_override() -> Option<ThemeMode> {
    match env::var("OC_THEME").ok()?.to_ascii_lowercase().as_str() {
        "light" => Some(ThemeMode::Light),
        "dark" => Some(ThemeMode::Dark),
        _ => None,
    }
}

fn theme_mode_from_colorfgbg() -> Option<ThemeMode> {
    let colorfgbg = env::var("COLORFGBG").ok()?;
    let background = colorfgbg.split(';').next_back()?.parse::<u8>().ok()?;

    Some(if background <= 6 || background == 8 {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    })
}

fn theme_mode_from_osc_11() -> Option<ThemeMode> {
    let mut tty = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(O_NONBLOCK)
        .open("/dev/tty")
        .ok()?;

    clear_pending_tty_bytes(&mut tty);
    tty.write_all(background_query_sequence().as_bytes()).ok()?;
    tty.flush().ok()?;

    let response = read_osc_response(&mut tty, OSC_QUERY_TIMEOUT)?;
    let (red, green, blue) = parse_background_color(&response)?;

    Some(if perceived_luma(red, green, blue) >= 0.5 {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    })
}

fn background_query_sequence() -> String {
    let osc = "\x1b]11;?\x07";
    if env::var_os("TMUX").is_some() {
        format!("\x1bPtmux;\x1b{osc}\x1b\\")
    } else {
        String::from(osc)
    }
}

fn clear_pending_tty_bytes(tty: &mut std::fs::File) {
    let mut buffer = [0u8; 256];
    while tty.read(&mut buffer).is_ok_and(|count| count > 0) {}
}

fn read_osc_response(tty: &mut std::fs::File, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    let mut buffer = Vec::new();

    while Instant::now() < deadline {
        let mut chunk = [0u8; 256];
        match tty.read(&mut chunk) {
            Ok(0) => std::thread::sleep(Duration::from_millis(10)),
            Ok(count) => {
                buffer.extend_from_slice(&chunk[..count]);
                if buffer.ends_with(b"\x07") || buffer.windows(2).any(|window| window == b"\x1b\\")
                {
                    return String::from_utf8(buffer).ok();
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    }

    None
}

fn parse_background_color(response: &str) -> Option<(u16, u16, u16)> {
    let start = response.find("11;")? + 3;
    let payload = response[start..]
        .trim_start_matches("rgb:")
        .trim_start_matches('#')
        .trim_end_matches('\u{7}')
        .trim_end_matches("\u{1b}\\")
        .trim_end_matches('\u{1b}')
        .trim_end_matches('\\');

    if let Some((red, rest)) = payload.split_once('/') {
        let (green, blue) = rest.split_once('/')?;
        return Some((
            u16::from_str_radix(red, 16).ok()?,
            u16::from_str_radix(green, 16).ok()?,
            u16::from_str_radix(blue, 16).ok()?,
        ));
    }

    if payload.len() == 6 {
        return Some((
            u16::from_str_radix(&payload[0..2], 16).ok()? * 257,
            u16::from_str_radix(&payload[2..4], 16).ok()? * 257,
            u16::from_str_radix(&payload[4..6], 16).ok()? * 257,
        ));
    }

    None
}

fn perceived_luma(red: u16, green: u16, blue: u16) -> f32 {
    let normalize = |value: u16| value as f32 / 65535.0;
    0.2126 * normalize(red) + 0.7152 * normalize(green) + 0.0722 * normalize(blue)
}

fn light_theme() -> Theme {
    Theme {
        outer_bg: Color::Reset,
        panel_bg: Color::Gray,
        panel_text: Color::Black,
        muted_text: Color::DarkGray,
        accent: Color::Blue,
        success: Color::Green,
        warning: Color::Yellow,
        selection_bg: Color::Blue,
        selection_text: Color::White,
        button_bg: Color::White,
        disabled_text: Color::DarkGray,
        help_text: Color::DarkGray,
    }
}

fn dark_theme() -> Theme {
    Theme {
        outer_bg: Color::Reset,
        panel_bg: Color::DarkGray,
        panel_text: Color::White,
        muted_text: Color::Gray,
        accent: Color::Cyan,
        success: Color::Green,
        warning: Color::Yellow,
        selection_bg: Color::Cyan,
        selection_text: Color::Black,
        button_bg: Color::Black,
        disabled_text: Color::Gray,
        help_text: Color::Gray,
    }
}
