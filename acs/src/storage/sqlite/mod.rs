//! SQLite-backed implementations of [`WorkflowStore`] and [`WorkflowRunStore`].
//!
//! Concurrency model
//! -----------------
//! Each store owns an `Arc<std::sync::Mutex<rusqlite::Connection>>`. Every
//! trait method offloads the rusqlite work to `tokio::task::spawn_blocking`
//! so the synchronous DB calls do not block the tokio runtime. The mutex is
//! a `std::sync::Mutex` (not `tokio::sync::Mutex`) because rusqlite is sync —
//! we never await while holding the lock.

mod row_helpers;
pub mod schema;
pub mod workflow_runs;
pub mod workflows;

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::errors::AcsError;

pub use schema::{apply_pragmas, apply_schema};
pub use workflow_runs::SqliteWorkflowRunStore;
pub use workflows::SqliteWorkflowStore;

/// A handle to an open SQLite database, shared between the workflow and
/// workflow-run stores. Cheap to clone (it's an `Arc` inside).
#[derive(Clone)]
pub struct SqliteDb {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteDb {
    /// Build a `SqliteDb` from an already-opened connection. The caller is
    /// responsible for having applied pragmas + schema.
    pub fn from_connection(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    /// Run a closure on the underlying connection inside
    /// `tokio::task::spawn_blocking`, so synchronous rusqlite work does not
    /// stall the tokio runtime. The closure receives an exclusive
    /// `&mut Connection` (the std mutex is held for the duration; we never
    /// await while holding it). A `JoinError` from the blocking task is
    /// translated to [`AcsError::Internal`].
    pub(super) async fn with_conn<F, T>(&self, f: F) -> Result<T, AcsError>
    where
        F: FnOnce(&mut Connection) -> Result<T, AcsError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().expect("sqlite mutex poisoned");
            f(&mut guard)
        })
        .await
        .map_err(|e| AcsError::Internal(format!("blocking task failed: {}", e)))?
    }
}

pub(crate) fn open_with_schema(path: &Path) -> Result<Connection, AcsError> {
    let conn = Connection::open(path)
        .map_err(|e| AcsError::Storage(format!("Failed to open SQLite at {:?}: {}", path, e)))?;
    apply_pragmas(&conn)?;
    apply_schema(&conn)?;
    Ok(conn)
}

/// Open (or create) the SQLite database at `path`, apply pragmas, and apply
/// the schema. Returns a [`SqliteDb`] suitable for constructing the stores.
///
/// The parent directory of `path` must already exist; this helper does not
/// create it.
pub fn init_db(path: &Path) -> Result<SqliteDb, AcsError> {
    let conn = open_with_schema(path)?;
    Ok(SqliteDb::from_connection(conn))
}

/// Open an in-memory SQLite database with the schema applied. Intended for
/// unit tests; production code should always use [`init_db`] with a real path.
pub fn init_in_memory_db() -> Result<SqliteDb, AcsError> {
    let conn = Connection::open_in_memory()
        .map_err(|e| AcsError::Storage(format!("Failed to open in-memory SQLite: {}", e)))?;
    apply_pragmas(&conn)?;
    apply_schema(&conn)?;
    Ok(SqliteDb::from_connection(conn))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_init_db_creates_file() {
        let tmp = TempDir::new().expect("tmp");
        let path = tmp.path().join("acs.sqlite");
        assert!(!path.exists());
        let _db = init_db(&path).expect("init");
        assert!(path.exists(), "init_db must create the file");
    }

    #[test]
    fn test_init_db_is_idempotent() {
        let tmp = TempDir::new().expect("tmp");
        let path = tmp.path().join("acs.sqlite");
        let _db1 = init_db(&path).expect("first init");
        // Drop and re-open: schema already there, must succeed.
        let _db2 = init_db(&path).expect("second init must succeed");
    }

    #[test]
    fn test_in_memory_db_works() {
        let _db = init_in_memory_db().expect("in-memory");
    }
}
