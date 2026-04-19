use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, ErrorCode, OpenFlags, params};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootSessionIdLookup {
    Available(BTreeSet<String>),
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct OpenCodeDb {
    path: PathBuf,
}

impl OpenCodeDb {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn root_session_ids_for_directory(&self, directory: &Path) -> Result<RootSessionIdLookup> {
        if !self.path.exists() {
            return Ok(RootSessionIdLookup::Available(BTreeSet::new()));
        }

        let connection =
            match Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
                Ok(connection) => connection,
                Err(error) => return self.handle_open_error(error),
            };

        let mut statement = match connection
            .prepare("SELECT id FROM session WHERE directory = ?1 AND parent_id IS NULL")
        {
            Ok(statement) => statement,
            Err(error) if is_missing_session_table_error(&error) => {
                return Ok(RootSessionIdLookup::Available(BTreeSet::new()));
            }
            Err(error) if is_unavailable_error(&error) => {
                return Ok(RootSessionIdLookup::Unavailable);
            }
            Err(error) => {
                return Err(anyhow!(error)).with_context(|| {
                    format!(
                        "failed to prepare OpenCode session query against {}",
                        self.path.display()
                    )
                });
            }
        };

        let rows = match statement.query_map(params![directory.display().to_string()], |row| {
            row.get::<_, String>(0)
        }) {
            Ok(rows) => rows,
            Err(error) if is_unavailable_error(&error) => {
                return Ok(RootSessionIdLookup::Unavailable);
            }
            Err(error) => {
                return Err(anyhow!(error)).with_context(|| {
                    format!(
                        "failed to query OpenCode sessions from {}",
                        self.path.display()
                    )
                });
            }
        };

        let mut ids = BTreeSet::new();
        for row in rows {
            ids.insert(row.with_context(|| {
                format!(
                    "failed to decode OpenCode session ID row from {}",
                    self.path.display()
                )
            })?);
        }

        Ok(RootSessionIdLookup::Available(ids))
    }

    fn handle_open_error(&self, error: rusqlite::Error) -> Result<RootSessionIdLookup> {
        if is_unavailable_error(&error) {
            return Ok(RootSessionIdLookup::Unavailable);
        }

        Err(anyhow!(error))
            .with_context(|| format!("failed to open OpenCode database {}", self.path.display()))
    }
}

fn is_unavailable_error(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(sqlite_error, _)
            if matches!(sqlite_error.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    )
}

fn is_missing_session_table_error(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(_, Some(message)) if message.contains("no such table: session"))
}
