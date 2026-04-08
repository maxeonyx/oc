use anyhow::{Context, Result};
use rusqlite::Connection;

const SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS sessions (
        id INTEGER PRIMARY KEY NOT NULL,
        name TEXT NOT NULL UNIQUE,
        directory TEXT NOT NULL,
        opencode_session_id TEXT,
        opencode_args TEXT NOT NULL
    ) STRICT;
";

pub fn ensure(connection: &Connection) -> Result<()> {
    connection
        .execute_batch(SCHEMA_SQL)
        .context("failed to initialize session database schema")
}
