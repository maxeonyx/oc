use crate::session::SavedSession;
use std::path::Path;

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

    format!("{prefix}/…")
}

pub fn format_column_row(
    id: &str,
    name: &str,
    status: &str,
    memory: &str,
    directory: &str,
    widths: &ColumnWidths,
) -> String {
    format!(
        "{:<id_width$}  {:<name_width$}  {:<status_width$}  {:<memory_width$}  {}",
        id,
        name,
        status,
        memory,
        directory,
        id_width = widths.id,
        name_width = widths.name,
        status_width = widths.status,
        memory_width = widths.memory,
    )
}

#[derive(Debug, Clone, Copy)]
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
