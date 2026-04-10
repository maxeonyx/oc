use crate::session::SavedSession;
use std::path::Path;
use unicode_width::UnicodeWidthStr;

pub fn format_memory(bytes: u64) -> String {
    if bytes == 0 {
        return String::from("0 MiB");
    }

    format!("{} MiB", bytes / 1024 / 1024)
}

pub fn abbreviate_directory(saved_session: &SavedSession) -> String {
    let directory = saved_session.directory.display().to_string();
    let Some(basename) = basename(&saved_session.directory) else {
        return directory;
    };

    if basename != saved_session.name {
        return directory;
    }

    let prefix = saved_session
        .directory
        .parent()
        .map(|parent| parent.display().to_string())
        .filter(|parent| !parent.is_empty())
        .unwrap_or_else(|| String::from("."));

    if prefix == "/" {
        String::from("/…")
    } else {
        format!("{prefix}/…")
    }
}

pub fn format_column_row(
    id: &str,
    name: &str,
    status: &str,
    memory: &str,
    directory: &str,
    widths: &ColumnWidths,
) -> String {
    [
        pad_to_display_width(id, widths.id),
        String::from("  "),
        pad_to_display_width(name, widths.name),
        String::from("  "),
        pad_to_display_width(status, widths.status),
        String::from("  "),
        pad_to_display_width(memory, widths.memory),
        String::from("  "),
        String::from(directory),
    ]
    .concat()
}

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn pad_to_display_width(text: &str, target_width: usize) -> String {
    let padding_width = target_width.saturating_sub(display_width(text));
    format!("{text}{}", " ".repeat(padding_width))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnWidths {
    pub id: usize,
    pub name: usize,
    pub status: usize,
    pub memory: usize,
}

fn basename(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(String::from)
}
