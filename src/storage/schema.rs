use anyhow::{Context, Result};
use rusqlite::Connection;

const SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS sessions (
        id INTEGER PRIMARY KEY NOT NULL,
        name TEXT NOT NULL UNIQUE,
        directory TEXT NOT NULL,
        opencode_session_id TEXT,
        opencode_args TEXT NOT NULL,
        last_used_at INTEGER NOT NULL DEFAULT 0
    ) STRICT;
";

pub fn ensure(connection: &Connection) -> Result<()> {
    connection
        .execute_batch(SCHEMA_SQL)
        .context("failed to initialize session database schema")?;

    ensure_last_used_at_column(connection)
}

fn ensure_last_used_at_column(connection: &Connection) -> Result<()> {
    let has_last_used_at = connection
        .prepare("PRAGMA table_info(sessions)")
        .context("failed to prepare sessions schema inspection query")?
        .query_map([], |row| row.get::<_, String>(1))
        .context("failed to inspect sessions schema columns")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to decode sessions schema columns")?
        .iter()
        .any(|column| column == "last_used_at");

    if !has_last_used_at {
        connection
            .execute(
                "ALTER TABLE sessions ADD COLUMN last_used_at INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .context("failed to add last_used_at column to sessions table")?;
    }

    Ok(())
}
