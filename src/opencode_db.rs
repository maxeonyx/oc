use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, ErrorCode, OpenFlags, params};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::directory_identity::normalize_directory_for_match;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootSessionIdLookup {
    Available(BTreeSet<String>),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessSessionLookup {
    Available(ProcessSessionRowLookup),
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSessionTableLookup {
    Available,
    Missing,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessSessionRowLookup {
    /// `process_session` is absent, so older OpenCode compatibility fallback is still allowed.
    TableMissing,
    /// `process_session` exists but has no row for this pid, so directory fallback would risk wrong-session capture.
    RowMissing,
    /// `process_session` exists and matches the pid, but the session id is not populated yet.
    SessionIdMissing,
    SessionId(String),
    /// `process_session` exists, but the row no longer describes the current process for this pid.
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessSessionRow {
    proc_start_ticks: u64,
    session_id: Option<String>,
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

        let normalized_directory = normalize_directory_for_match(directory);

        let connection =
            match Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
                Ok(connection) => connection,
                Err(error) => return self.handle_open_error(error),
            };

        let mut statement =
            match connection.prepare("SELECT id, directory FROM session WHERE parent_id IS NULL") {
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

        let rows = match statement.query_map(params![], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
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
            let (id, row_directory) = row.with_context(|| {
                format!(
                    "failed to decode OpenCode session ID row from {}",
                    self.path.display()
                )
            })?;

            if normalize_directory_for_match(Path::new(&row_directory)) == normalized_directory {
                ids.insert(id);
            }
        }

        Ok(RootSessionIdLookup::Available(ids))
    }

    pub fn process_session_id_for_pid(&self, pid: u32) -> Result<ProcessSessionLookup> {
        if !self.path.exists() {
            return Ok(ProcessSessionLookup::Available(
                ProcessSessionRowLookup::RowMissing,
            ));
        }

        let connection =
            match Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
                Ok(connection) => connection,
                Err(error) => return self.handle_process_session_open_error(error),
            };

        match query_process_session_table_lookup(&connection, &self.path)? {
            ProcessSessionTableLookup::Available => {}
            ProcessSessionTableLookup::Missing => {
                return Ok(ProcessSessionLookup::Available(
                    ProcessSessionRowLookup::TableMissing,
                ));
            }
            ProcessSessionTableLookup::Unavailable => return Ok(ProcessSessionLookup::Unavailable),
        }

        let row = match connection.query_row(
            "SELECT proc_start_ticks, session_id FROM process_session WHERE pid = ?1",
            params![pid],
            |row| {
                Ok(ProcessSessionRow {
                    proc_start_ticks: row.get(0)?,
                    session_id: row.get(1)?,
                })
            },
        ) {
            Ok(row) => row,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Ok(ProcessSessionLookup::Available(
                    ProcessSessionRowLookup::RowMissing,
                ));
            }
            Err(error) if is_unavailable_error(&error) => {
                return Ok(ProcessSessionLookup::Unavailable);
            }
            Err(error) => {
                return Err(anyhow!(error)).with_context(|| {
                    format!(
                        "failed to query process_session row for pid {} from {}",
                        pid,
                        self.path.display()
                    )
                });
            }
        };

        if !process_start_ticks_match(pid, row.proc_start_ticks)? {
            return Ok(ProcessSessionLookup::Available(
                ProcessSessionRowLookup::Stale,
            ));
        }

        Ok(ProcessSessionLookup::Available(match row.session_id {
            Some(session_id) => {
                let session_id = resolve_root_session_id(&connection, &self.path, &session_id)?;
                ProcessSessionRowLookup::SessionId(session_id)
            }
            None => ProcessSessionRowLookup::SessionIdMissing,
        }))
    }

    pub fn process_session_table_lookup(&self) -> Result<ProcessSessionTableLookup> {
        if !self.path.exists() {
            return Ok(ProcessSessionTableLookup::Missing);
        }

        let connection =
            match Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
                Ok(connection) => connection,
                Err(error) => return self.handle_process_session_table_open_error(error),
            };

        query_process_session_table_lookup(&connection, &self.path)
    }

    fn handle_open_error(&self, error: rusqlite::Error) -> Result<RootSessionIdLookup> {
        if is_unavailable_error(&error) {
            return Ok(RootSessionIdLookup::Unavailable);
        }

        Err(anyhow!(error))
            .with_context(|| format!("failed to open OpenCode database {}", self.path.display()))
    }

    fn handle_process_session_open_error(
        &self,
        error: rusqlite::Error,
    ) -> Result<ProcessSessionLookup> {
        if is_unavailable_error(&error) {
            return Ok(ProcessSessionLookup::Unavailable);
        }

        Err(anyhow!(error))
            .with_context(|| format!("failed to open OpenCode database {}", self.path.display()))
    }

    fn handle_process_session_table_open_error(
        &self,
        error: rusqlite::Error,
    ) -> Result<ProcessSessionTableLookup> {
        if is_unavailable_error(&error) {
            return Ok(ProcessSessionTableLookup::Unavailable);
        }

        Err(anyhow!(error))
            .with_context(|| format!("failed to open OpenCode database {}", self.path.display()))
    }
}

fn query_process_session_table_lookup(
    connection: &Connection,
    path: &Path,
) -> Result<ProcessSessionTableLookup> {
    match connection.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'process_session'",
        params![],
        |_| Ok(()),
    ) {
        Ok(()) => Ok(ProcessSessionTableLookup::Available),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ProcessSessionTableLookup::Missing),
        Err(error) if is_unavailable_error(&error) => Ok(ProcessSessionTableLookup::Unavailable),
        Err(error) => Err(anyhow!(error)).with_context(|| {
            format!(
                "failed to query process_session table presence in {}",
                path.display()
            )
        }),
    }
}

fn resolve_root_session_id(connection: &Connection, path: &Path, session_id: &str) -> Result<String> {
    let original_session_id = session_id.to_string();
    let mut current_session_id = original_session_id.clone();
    let mut first_lookup = true;

    loop {
        match connection.query_row(
            "SELECT parent_id FROM session WHERE id = ?1",
            params![&current_session_id],
            |row| row.get::<_, Option<String>>(0),
        ) {
            Ok(Some(parent_id)) => {
                current_session_id = parent_id;
                first_lookup = false;
            }
            Ok(None) => return Ok(current_session_id),
            Err(rusqlite::Error::QueryReturnedNoRows) if first_lookup => {
                return Ok(original_session_id);
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(anyhow!(
                    "OpenCode session parent chain is broken at '{}'",
                    current_session_id
                ))
                .with_context(|| {
                    format!(
                        "failed to resolve OpenCode root session ID from {}",
                        path.display()
                    )
                });
            }
            Err(error) if is_missing_session_table_error(&error) => return Ok(original_session_id),
            Err(error) if is_unavailable_error(&error) => {
                return Err(anyhow!(error)).with_context(|| {
                    format!(
                        "failed to resolve OpenCode root session ID from {}",
                        path.display()
                    )
                });
            }
            Err(error) => {
                return Err(anyhow!(error)).with_context(|| {
                    format!(
                        "failed to resolve OpenCode root session ID from {}",
                        path.display()
                    )
                });
            }
        }
    }
}

fn process_start_ticks_match(pid: u32, expected_ticks: u64) -> Result<bool> {
    let stat_path = PathBuf::from(format!("/proc/{pid}/stat"));
    let contents = match fs::read_to_string(&stat_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to read process stat file {}", stat_path.display())
            });
        }
    };

    Ok(parse_proc_start_ticks(&contents) == Some(expected_ticks))
}

fn parse_proc_start_ticks(stat_contents: &str) -> Option<u64> {
    stat_contents.split_whitespace().nth(21)?.parse().ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_root_session_id_returns_root_for_nested_subagent_session() {
        let connection = Connection::open_in_memory().expect("in-memory sqlite should open");
        connection
            .execute_batch(
                "
                CREATE TABLE session (
                    id TEXT PRIMARY KEY NOT NULL,
                    directory TEXT NOT NULL,
                    parent_id TEXT,
                    time_created INTEGER NOT NULL,
                    time_updated INTEGER NOT NULL
                );
                INSERT INTO session (id, directory, parent_id, time_created, time_updated)
                VALUES
                    ('ses_root', '/tmp/project', NULL, 1, 1),
                    ('ses_child', '/tmp/project', 'ses_root', 1, 1),
                    ('ses_grandchild', '/tmp/project', 'ses_child', 1, 1);
                ",
            )
            .expect("session schema should initialize");

        let resolved =
            resolve_root_session_id(&connection, Path::new("/tmp/opencode.db"), "ses_grandchild")
                .expect("nested session should resolve to root");

        assert_eq!(resolved, "ses_root");
    }

    #[test]
    fn resolve_root_session_id_preserves_unknown_session_id() {
        let connection = Connection::open_in_memory().expect("in-memory sqlite should open");
        connection
            .execute_batch(
                "
                CREATE TABLE session (
                    id TEXT PRIMARY KEY NOT NULL,
                    directory TEXT NOT NULL,
                    parent_id TEXT,
                    time_created INTEGER NOT NULL,
                    time_updated INTEGER NOT NULL
                );
                ",
            )
            .expect("session schema should initialize");

        let resolved =
            resolve_root_session_id(&connection, Path::new("/tmp/opencode.db"), "ses_missing")
                .expect("unknown session should be preserved");

        assert_eq!(resolved, "ses_missing");
    }
}
