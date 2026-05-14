use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::directory_identity::normalize_directory_for_storage;

const SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS sessions (
        id INTEGER PRIMARY KEY NOT NULL,
        name TEXT NOT NULL UNIQUE,
        directory TEXT NOT NULL,
        opencode_session_id TEXT,
        last_used_at INTEGER NOT NULL DEFAULT 0
    ) STRICT;
";

pub fn ensure(connection: &Connection) -> Result<()> {
    connection
        .execute_batch(SCHEMA_SQL)
        .context("failed to initialize session database schema")?;

    ensure_last_used_at_column(connection)?;
    drop_opencode_args_column(connection)?;
    expand_tilde_directories(connection)
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

fn expand_tilde_directories(connection: &Connection) -> Result<()> {
    let mut statement = connection
        .prepare("SELECT id, directory FROM sessions")
        .context("failed to prepare sessions directory migration query")?;

    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .context("failed to query sessions for directory migration")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to decode sessions for directory migration")?;

    for (id, directory) in rows {
        if !directory.starts_with('~') {
            continue;
        }

        let normalized_directory =
            normalize_directory_for_storage(std::path::Path::new(&directory))?
                .display()
                .to_string();

        if normalized_directory == directory {
            continue;
        }

        connection
            .execute(
                "UPDATE sessions SET directory = ?1 WHERE id = ?2",
                rusqlite::params![normalized_directory, id],
            )
            .with_context(|| format!("failed to migrate directory for session row {id}"))?;
    }

    Ok(())
}

fn drop_opencode_args_column(connection: &Connection) -> Result<()> {
    let columns = connection
        .prepare("PRAGMA table_info(sessions)")
        .context("failed to prepare sessions schema inspection query for launch-arg cleanup")?
        .query_map([], |row| row.get::<_, String>(1))
        .context("failed to inspect sessions schema columns for launch-arg cleanup")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to decode sessions schema columns for launch-arg cleanup")?;

    if !columns.iter().any(|column| column == "opencode_args") {
        return Ok(());
    }

    connection
        .execute_batch(
            "
            BEGIN;
            ALTER TABLE sessions RENAME TO sessions_old;
            CREATE TABLE sessions (
                id INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL UNIQUE,
                directory TEXT NOT NULL,
                opencode_session_id TEXT,
                last_used_at INTEGER NOT NULL DEFAULT 0
            ) STRICT;
            INSERT INTO sessions (id, name, directory, opencode_session_id, last_used_at)
            SELECT id, name, directory, opencode_session_id, COALESCE(last_used_at, 0)
            FROM sessions_old;
            DROP TABLE sessions_old;
            COMMIT;
            ",
        )
        .context("failed to rebuild sessions table without persisted launch args")?;

    Ok(())
}
