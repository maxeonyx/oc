use crate::session::SessionListEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListRow {
    pub name: String,
    pub status: String,
    pub directory: String,
    pub session_id: Option<String>,
    pub saved_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColumnWidths {
    name: usize,
    status: usize,
    directory: usize,
    session_id: usize,
    saved_id: usize,
}

impl SessionListRow {
    fn from_entry(entry: SessionListEntry) -> Self {
        let saved_session = entry.saved_session;

        Self {
            name: saved_session.name,
            status: entry.status.public_label().to_string(),
            directory: saved_session.directory.display().to_string(),
            session_id: saved_session.opencode_session_id,
            saved_id: saved_session.id,
        }
    }

    fn session_id_display(&self) -> &str {
        self.session_id.as_deref().unwrap_or("(none)")
    }
}

pub fn rows_from_entries(entries: Vec<SessionListEntry>) -> Vec<SessionListRow> {
    entries
        .into_iter()
        .map(SessionListRow::from_entry)
        .collect()
}

pub fn render_table(rows: &[SessionListRow]) -> String {
    let widths = column_widths(rows);
    let mut lines = Vec::with_capacity(rows.len() + 4);
    lines.push(format_row(
        "NAME",
        "STATUS",
        "DIRECTORY",
        "SESSION ID",
        "ID",
        widths,
    ));

    if rows.is_empty() {
        lines.push(format_row("(no sessions)", "-", "-", "-", "-", widths));
    } else {
        for row in rows {
            lines.push(format_row(
                &row.name,
                &row.status,
                &row.directory,
                row.session_id_display(),
                &row.saved_id.to_string(),
                widths,
            ));
        }
    }

    lines.push(summary_line(rows));
    format!("{}\n", lines.join("\n"))
}

pub fn render_json(rows: &[SessionListRow]) -> String {
    let values = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "name": row.name,
                "status": row.status,
                "directory": row.directory,
                "session_id": row.session_id,
                "id": row.saved_id,
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string_pretty(&values).expect("session list JSON serialization should succeed")
        + "\n"
}

fn column_widths(rows: &[SessionListRow]) -> ColumnWidths {
    rows.iter().fold(
        ColumnWidths {
            name: "NAME".len().max("(no sessions)".len()),
            status: "STATUS".len(),
            directory: "DIRECTORY".len(),
            session_id: "SESSION ID".len(),
            saved_id: "ID".len(),
        },
        |widths, row| ColumnWidths {
            name: widths.name.max(row.name.len()),
            status: widths.status.max(row.status.len()),
            directory: widths.directory.max(row.directory.len()),
            session_id: widths.session_id.max(row.session_id_display().len()),
            saved_id: widths.saved_id.max(row.saved_id.to_string().len()),
        },
    )
}

fn format_row(
    name: &str,
    status: &str,
    directory: &str,
    session_id: &str,
    saved_id: &str,
    widths: ColumnWidths,
) -> String {
    format!(
        "{name:<name_width$}  {status:<status_width$}  {directory:<directory_width$}  {session_id:<session_id_width$}  {saved_id:>saved_id_width$}",
        name_width = widths.name,
        status_width = widths.status,
        directory_width = widths.directory,
        session_id_width = widths.session_id,
        saved_id_width = widths.saved_id,
    )
}

fn summary_line(rows: &[SessionListRow]) -> String {
    let attached = rows.iter().filter(|row| row.status == "attached").count();
    let detached = rows.iter().filter(|row| row.status == "detached").count();
    let saved = rows.iter().filter(|row| row.status == "saved").count();

    format!(
        "{} sessions: {} attached, {} detached, {} saved",
        rows.len(),
        attached,
        detached,
        saved
    )
}
