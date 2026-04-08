use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Transaction, params};
use std::fs;
use std::path::Path;

use crate::session::{NewSessionAlias, SavedSession};

use super::schema;

pub struct SessionStore {
    connection: Connection,
}

impl SessionStore {
    pub fn open(db_path: &Path) -> Result<Self> {
        ensure_parent_dir(db_path)?;

        let connection = Connection::open(db_path)
            .with_context(|| format!("failed to open session database {}", db_path.display()))?;

        schema::ensure(&connection).with_context(|| {
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

        let id = next_session_id(&transaction)?;
        let directory = alias.directory.display().to_string();
        let serialized_opencode_args = serialize_opencode_args(&alias.opencode_args)?;

        insert_alias_row(
            &transaction,
            id,
            &alias.name,
            &directory,
            &serialized_opencode_args,
        )?;

        transaction
            .commit()
            .context("failed to commit saved session alias")?;

        Ok(SavedSession {
            id,
            name: alias.name,
            directory: alias.directory,
            opencode_session_id: None,
            opencode_args: alias.opencode_args,
        })
    }

    pub fn remove_alias(&mut self, name: &str) -> Result<()> {
        let deleted_rows = self
            .connection
            .execute("DELETE FROM sessions WHERE name = ?1", params![name])
            .with_context(|| format!("failed to delete session alias '{name}' from storage"))?;

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

fn next_session_id(transaction: &Transaction<'_>) -> Result<i64> {
    // IDs are dense and gap-filling: return 1 when empty, otherwise the first
    // missing positive integer, or the next value after the current highest ID.
    transaction
        .query_row(
            "
            SELECT COALESCE(
                (
                    SELECT 1
                    WHERE NOT EXISTS (SELECT 1 FROM sessions WHERE id = 1)
                ),
                (
                    SELECT current.id + 1
                    FROM sessions AS current
                    LEFT JOIN sessions AS next ON next.id = current.id + 1
                    WHERE next.id IS NULL
                    ORDER BY current.id
                    LIMIT 1
                )
            )
            ",
            [],
            |row| row.get(0),
        )
        .context("failed to allocate next dense session ID")
}

fn serialize_opencode_args(opencode_args: &[String]) -> Result<String> {
    serde_json::to_string(opencode_args).context("failed to serialize OpenCode args")
}

fn insert_alias_row(
    transaction: &Transaction<'_>,
    id: i64,
    name: &str,
    directory: &str,
    serialized_opencode_args: &str,
) -> Result<()> {
    match transaction.execute(
        "
        INSERT INTO sessions (id, name, directory, opencode_session_id, opencode_args)
        VALUES (?1, ?2, ?3, NULL, ?4)
        ",
        params![id, name, directory, serialized_opencode_args],
    ) {
        Ok(_) => Ok(()),
        Err(error) => Err(map_insert_error(transaction, name, error)),
    }
}

fn map_insert_error(
    transaction: &Transaction<'_>,
    name: &str,
    error: rusqlite::Error,
) -> anyhow::Error {
    match error {
        rusqlite::Error::SqliteFailure(sqlite_error, _)
            if sqlite_error.code == ErrorCode::ConstraintViolation =>
        {
            match alias_name_exists(transaction, name) {
                Ok(true) => anyhow!("Session alias '{name}' already exists"),
                Ok(false) => anyhow!(
                    "Failed to save session alias '{name}' because storage rejected the row"
                ),
                Err(lookup_error) => lookup_error.context(format!(
                    "failed to determine why saving session alias '{name}' violated a storage constraint"
                )),
            }
        }
        other => anyhow!(other).context(format!("failed to insert session alias '{name}'")),
    }
}

fn alias_name_exists(transaction: &Transaction<'_>, name: &str) -> Result<bool> {
    let existing_name = transaction
        .query_row(
            "SELECT 1 FROM sessions WHERE name = ?1",
            params![name],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .with_context(|| {
            format!("failed to check whether session alias '{name}' already exists")
        })?;

    Ok(existing_name.is_some())
}
