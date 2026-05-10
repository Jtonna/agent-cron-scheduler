//! Shared row-parsing and serialisation helpers used by both
//! [`SqliteWorkflowStore`](super::workflows::SqliteWorkflowStore) and
//! [`SqliteWorkflowRunStore`](super::workflow_runs::SqliteWorkflowRunStore).
//!
//! Anything in this module is `pub(super)` — these are implementation details
//! of the SQLite storage layer, not part of the public API.

use chrono::{DateTime, Utc};
use rusqlite::Row;

use crate::errors::AcsError;
use crate::models::workflow::RunStatus;

/// Read a non-null TEXT column and parse it as an RFC3339 timestamp.
pub(super) fn parse_dt(row: &Row<'_>, col: &str) -> rusqlite::Result<DateTime<Utc>> {
    let s: String = row.get(col)?;
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

/// Read an optional TEXT column and parse it as an RFC3339 timestamp.
pub(super) fn parse_opt_dt(row: &Row<'_>, col: &str) -> rusqlite::Result<Option<DateTime<Utc>>> {
    let s: Option<String> = row.get(col)?;
    match s {
        None => Ok(None),
        Some(s) => DateTime::parse_from_rfc3339(&s)
            .map(|dt| Some(dt.with_timezone(&Utc)))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            }),
    }
}

/// Convert a `serde_json::Error` raised while decoding a column to a
/// `rusqlite::Error` so it can flow back through `query_map`/`query_row`.
pub(super) fn map_serde_err(e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

/// Convert a `uuid::Error` raised while decoding a column to a
/// `rusqlite::Error`.
pub(super) fn map_uuid_err(e: uuid::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

/// Serialize a `RunStatus` to its bare PascalCase string (e.g. "Running").
///
/// The enum is `#[serde(rename_all = "PascalCase")]` and has no data-bearing
/// variants, so its serde representation is always a `Value::String`. We
/// unwrap that to avoid quoting (`"Running"` → `Running`) before storing in
/// a TEXT column.
pub(super) fn run_status_str(s: &RunStatus) -> Result<String, AcsError> {
    match serde_json::to_value(s).map_err(|e| AcsError::Storage(e.to_string()))? {
        serde_json::Value::String(s) => Ok(s),
        other => Err(AcsError::Storage(format!(
            "RunStatus did not serialize to a string: {}",
            other
        ))),
    }
}
