//! m008_add_workflow_deleted — soft-delete support on `workflows`:
//! add the `deleted` column and replace the inline UNIQUE constraint on
//! `name` with a partial unique index over live rows.
//!
//! DELETE /api/workflows/{id} is a soft delete: the workflow row and all of
//! its workflow_runs are kept (the run rows are the cost/token record) and
//! the workflow is flagged deleted = 1, keeping its name verbatim. Name
//! uniqueness then holds among LIVE workflows only, so the inline UNIQUE on
//! workflows.name (which SQLite cannot drop in place) is removed via a
//! standard table rebuild and replaced with a partial unique index.
//!
//! This migration opts into the runner's rebuild convention: the runner
//! disables PRAGMA foreign_keys before opening the transaction (the pragma
//! is a no-op inside one), runs PRAGMA foreign_key_check before committing
//! (rolling back on any violation), and re-enables foreign_keys after.
//!
//! Existing rows keep their names verbatim and get deleted = 0 via the
//! column default.

use milepost::{MigrateError, Migration, MigrationTx};

const SQL: &str = r#"
CREATE TABLE workflows_new (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    version             INTEGER NOT NULL,
    schedule            TEXT NOT NULL,
    timezone            TEXT,
    schedule_mode       TEXT NOT NULL,
    enabled             INTEGER NOT NULL,
    steps_json          TEXT NOT NULL,
    default_input       TEXT,
    working_dir         TEXT,
    env_vars            TEXT,
    allow_concurrent    INTEGER NOT NULL,
    on_failure          TEXT NOT NULL,
    last_run_at         TEXT,
    last_run_status     TEXT,
    last_run_id         TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    is_favorited        INTEGER NOT NULL DEFAULT 0,
    deleted             INTEGER NOT NULL DEFAULT 0
);

INSERT INTO workflows_new (
    id, name, version, schedule, timezone, schedule_mode, enabled,
    steps_json, default_input, working_dir, env_vars,
    allow_concurrent, on_failure, last_run_at, last_run_status,
    last_run_id, created_at, updated_at, is_favorited
)
SELECT
    id, name, version, schedule, timezone, schedule_mode, enabled,
    steps_json, default_input, working_dir, env_vars,
    allow_concurrent, on_failure, last_run_at, last_run_status,
    last_run_id, created_at, updated_at, is_favorited
FROM workflows;

DROP TABLE workflows;

ALTER TABLE workflows_new RENAME TO workflows;

CREATE UNIQUE INDEX idx_workflows_name_live
    ON workflows(name) WHERE deleted = 0;
"#;

pub(crate) struct AddWorkflowDeleted;

impl Migration for AddWorkflowDeleted {
    fn name(&self) -> &'static str {
        "m008_add_workflow_deleted"
    }

    fn rebuild(&self) -> bool {
        true
    }

    fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError> {
        tx.execute_batch(SQL)
    }
}
