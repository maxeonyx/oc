use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, ErrorCode, OptionalExtension, Transaction, params};
use std::fs;
use std::path::Path;

use crate::session::{NewSessionAlias, SavedSession, SessionRef};

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
        insert_alias_row(&transaction, id, &alias)?;

        transaction
            .commit()
            .context("failed to commit saved session alias")?;

        Ok(SavedSession {
            id,
            name: alias.name,
            directory: alias.directory,
            opencode_session_id: alias.opencode_session_id,
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

    pub fn resolve_session_ref(&self, session_ref: &SessionRef) -> Result<SavedSession> {
        match session_ref {
            SessionRef::NumericId(id) => self.find_by_id(*id),
            SessionRef::Name(name) => self.find_by_name(name),
        }
    }

    pub fn update_directory(&mut self, name: &str, directory: &Path) -> Result<()> {
        let updated_rows = self
            .connection
            .execute(
                "UPDATE sessions SET directory = ?1 WHERE name = ?2",
                params![directory.display().to_string(), name],
            )
            .with_context(|| {
                format!("failed to update directory for session alias '{name}' in storage")
            })?;

        if updated_rows == 0 {
            bail!("Session alias '{name}' not found");
        }

        Ok(())
    }

    pub fn save_imported_alias(&mut self, alias: NewSessionAlias) -> Result<Option<SavedSession>> {
        if let Ok(existing) = self.find_by_name(&alias.name) {
            if existing.directory == alias.directory
                && existing.opencode_session_id == alias.opencode_session_id
                && existing.opencode_args == alias.opencode_args
            {
                return Ok(None);
            }

            bail!(
                "Session alias '{}' already exists with different values",
                alias.name
            );
        }

        self.save_alias(alias).map(Some)
    }

    pub fn list_saved_sessions(&self) -> Result<Vec<SavedSession>> {
        let mut statement = self.prepare_saved_session_query(
            "
            SELECT id, name, directory, opencode_session_id, opencode_args
            FROM sessions
            ORDER BY id
            ",
            "failed to prepare saved session listing query",
        )?;

        statement
            .query_map([], map_saved_session_row)
            .context("failed to query saved sessions")?
            .collect::<Result<Vec<_>, _>>()
            .context("failed to decode saved session rows")
    }

    pub fn update_opencode_session_id(
        &mut self,
        name: &str,
        opencode_session_id: Option<&str>,
    ) -> Result<()> {
        let updated_rows = self
            .connection
            .execute(
                "UPDATE sessions SET opencode_session_id = ?1 WHERE name = ?2",
                params![opencode_session_id, name],
            )
            .with_context(|| {
                format!(
                    "failed to update OpenCode session ID for session alias '{name}' in storage"
                )
            })?;

        if updated_rows == 0 {
            bail!("Session alias '{name}' not found");
        }

        Ok(())
    }

    fn find_by_id(&self, id: i64) -> Result<SavedSession> {
        self.find_saved_session(
            "
            SELECT id, name, directory, opencode_session_id, opencode_args
            FROM sessions
            WHERE id = ?1
            ",
            params![id],
            &format!("failed to look up session with numeric ID {id}"),
        )?
        .ok_or_else(|| anyhow!("Session {id} not found"))
    }

    fn find_by_name(&self, name: &str) -> Result<SavedSession> {
        self.find_saved_session(
            "
            SELECT id, name, directory, opencode_session_id, opencode_args
            FROM sessions
            WHERE name = ?1
            ",
            params![name],
            &format!("failed to look up session alias '{name}'"),
        )?
        .ok_or_else(|| anyhow!("Session alias '{name}' not found"))
    }

    fn prepare_saved_session_query<'conn>(
        &'conn self,
        sql: &str,
        error_context: &str,
    ) -> Result<rusqlite::Statement<'conn>> {
        self.connection
            .prepare(sql)
            .context(error_context.to_string())
    }

    fn find_saved_session<P>(
        &self,
        sql: &str,
        params: P,
        error_context: &str,
    ) -> Result<Option<SavedSession>>
    where
        P: rusqlite::Params,
    {
        self.connection
            .query_row(sql, params, map_saved_session_row)
            .optional()
            .context(error_context.to_string())
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

fn deserialize_opencode_args(opencode_args_json: String) -> Result<Vec<String>> {
    serde_json::from_str(&opencode_args_json).with_context(|| {
        format!("failed to deserialize OpenCode args from stored JSON: {opencode_args_json}")
    })
}

fn insert_alias_row(transaction: &Transaction<'_>, id: i64, alias: &NewSessionAlias) -> Result<()> {
    let directory = alias.directory.display().to_string();
    let serialized_opencode_args = serialize_opencode_args(&alias.opencode_args)?;

    match transaction.execute(
        "
        INSERT INTO sessions (id, name, directory, opencode_session_id, opencode_args)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ",
        params![
            id,
            alias.name,
            directory,
            alias.opencode_session_id,
            serialized_opencode_args
        ],
    ) {
        Ok(_) => Ok(()),
        Err(error) => Err(map_insert_error(transaction, &alias.name, error)),
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

fn map_saved_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedSession> {
    let opencode_args_json = row.get::<_, String>(4)?;
    let opencode_args = deserialize_opencode_args(opencode_args_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::other(error.to_string())),
        )
    })?;

    Ok(SavedSession {
        id: row.get(0)?,
        name: row.get(1)?,
        directory: Path::new(&row.get::<_, String>(2)?).to_path_buf(),
        opencode_session_id: row.get(3)?,
        opencode_args,
    })
}
