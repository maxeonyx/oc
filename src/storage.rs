use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Transaction, params};
use std::fs;
use std::path::Path;

use crate::session::{NewSessionAlias, SavedSession};

const SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS sessions (
        id INTEGER PRIMARY KEY NOT NULL,
        name TEXT NOT NULL UNIQUE,
        directory TEXT NOT NULL,
        opencode_session_id TEXT,
        opencode_args TEXT NOT NULL
    ) STRICT;
";

pub struct SessionStore {
    connection: Connection,
}

impl SessionStore {
    pub fn open(db_path: &Path) -> Result<Self> {
        ensure_parent_dir(db_path)?;

        let connection = Connection::open(db_path)
            .with_context(|| format!("failed to open session database {}", db_path.display()))?;

        connection.execute_batch(SCHEMA_SQL).with_context(|| {
            format!(
                "failed to initialize session database {}",
                db_path.display()
            )
        })?;

        Ok(Self { connection })
    }

    pub fn save_alias(&mut self, alias: NewSessionAlias) -> Result<SavedSession> {
        let transaction = self
            .connection
            .transaction()
            .context("failed to start session save transaction")?;

        ensure_name_is_available(&transaction, &alias.name)?;

        let id = next_session_id(&transaction)?;
        let opencode_args_json = serde_json::to_string(&alias.opencode_args)
            .context("failed to serialize OpenCode args")?;

        transaction
            .execute(
                "
                INSERT INTO sessions (id, name, directory, opencode_session_id, opencode_args)
                VALUES (?1, ?2, ?3, NULL, ?4)
                ",
                params![
                    id,
                    alias.name,
                    alias.directory.display().to_string(),
                    opencode_args_json,
                ],
            )
            .map_err(map_insert_error)?;

        transaction
            .commit()
            .context("failed to commit saved session alias")?;

        Ok(SavedSession {
            id,
            name: alias.name,
            directory: alias.directory,
            opencode_session_id: None,
            opencode_args_json,
        })
    }

    pub fn remove_alias(&mut self, name: &str) -> Result<()> {
        let deleted_rows = self
            .connection
            .execute("DELETE FROM sessions WHERE name = ?1", params![name])
            .with_context(|| format!("failed to remove saved session alias '{name}'"))?;

        if deleted_rows == 0 {
            bail!("Session alias '{name}' not found");
        }

        Ok(())
    }
}

fn ensure_parent_dir(db_path: &Path) -> Result<()> {
    let Some(parent_dir) = db_path.parent() else {
        bail!(
            "Session database path {} has no parent directory",
            db_path.display()
        );
    };

    fs::create_dir_all(parent_dir)
        .with_context(|| format!("failed to create {}", parent_dir.display()))?;

    Ok(())
}

fn ensure_name_is_available(transaction: &Transaction<'_>, name: &str) -> Result<()> {
    let existing_name = transaction
        .query_row(
            "SELECT name FROM sessions WHERE name = ?1",
            params![name],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("failed to check for existing session alias")?;

    if existing_name.is_some() {
        bail!("Session alias '{name}' already exists");
    }

    Ok(())
}

fn next_session_id(transaction: &Transaction<'_>) -> Result<i64> {
    transaction
        .query_row(
            "
            SELECT CASE
                WHEN NOT EXISTS (SELECT 1 FROM sessions WHERE id = 1) THEN 1
                ELSE (
                    SELECT current.id + 1
                    FROM sessions AS current
                    LEFT JOIN sessions AS next ON next.id = current.id + 1
                    WHERE next.id IS NULL
                    ORDER BY current.id
                    LIMIT 1
                )
            END
            ",
            [],
            |row| row.get(0),
        )
        .context("failed to allocate next dense session ID")
}

fn map_insert_error(error: rusqlite::Error) -> anyhow::Error {
    match error {
        rusqlite::Error::SqliteFailure(sqlite_error, _)
            if sqlite_error.code == ErrorCode::ConstraintViolation =>
        {
            anyhow!("Session alias already exists")
        }
        other => anyhow!(other).context("failed to insert saved session alias"),
    }
}
