//! m000_baseline — fresh-install starting point.
//!
//! Creates the pre-m003-era database schema so that m003..m008 apply on top,
//! reproducing exactly the schema history an upgraded database went through.
//!
//! This migration opts into the runner's baseline convention: on a database
//! that already has both migration tracking and a schema, the runner records
//! a success row for it WITHOUT executing, so the baseline never runs
//! against an existing schema. It executes only when the schema is absent —
//! a brand-new database, or one where a previous startup died right after
//! the tracking table was created.
//!
//! Shape notes (deliberately historical — later migrations transform it):
//!   * workflows.name carries an inline UNIQUE          (removed by m008)
//!   * workflows.input_schema exists                    (dropped by m004)
//!   * workflows has no `deleted` column                (added by m008)
//!   * workflow_runs has no token columns               (added by m007)

use crate::{MigrateError, Migration, MigrationTx};

const SQL: &str = r#"
CREATE TABLE workflows (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL UNIQUE,
    version             INTEGER NOT NULL,
    schedule            TEXT NOT NULL,
    timezone            TEXT,
    schedule_mode       TEXT NOT NULL,
    enabled             INTEGER NOT NULL,
    steps_json          TEXT NOT NULL,
    input_schema        TEXT,
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
    is_favorited        INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE workflow_runs (
    run_id              TEXT PRIMARY KEY,
    workflow_id         TEXT NOT NULL,
    workflow_version    INTEGER NOT NULL,
    workflow_snapshot   TEXT NOT NULL,
    started_at          TEXT NOT NULL,
    finished_at         TEXT,
    status              TEXT NOT NULL,
    trigger_input       TEXT,
    steps_json          TEXT NOT NULL,
    total_cost_usd         REAL,
    total_duration_ms      INTEGER,
    FOREIGN KEY (workflow_id) REFERENCES workflows(id)
);

CREATE INDEX idx_workflow_runs_workflow_id_finished_at
    ON workflow_runs(workflow_id, finished_at);
CREATE INDEX idx_workflow_runs_finished_at
    ON workflow_runs(finished_at);
CREATE INDEX idx_workflow_runs_status
    ON workflow_runs(status);

CREATE TABLE meta (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);
"#;

pub(crate) struct Baseline;

impl Migration for Baseline {
    fn name(&self) -> &'static str {
        "m000_baseline"
    }

    fn baseline(&self) -> bool {
        true
    }

    fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError> {
        tx.execute_batch(SQL)
    }
}
