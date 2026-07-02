-- m000_baseline — fresh-install starting point.
--
-- Creates the pre-m003-era database schema so that m003..m008 apply on top,
-- reproducing exactly the schema history an upgraded database went through.
--
-- This migration carries the runner's `baseline` flag: it only EXECUTES on a
-- brand-new database (no schema_migrations table yet). On a database that
-- already has migration tracking (any v4.2.14 install), the runner records a
-- success row for it WITHOUT executing, so the baseline never runs against an
-- existing schema.
--
-- Shape notes (deliberately historical — later migrations transform it):
--   * workflows.name carries an inline UNIQUE          (removed by m008)
--   * workflows.input_schema exists                    (dropped by m004)
--   * workflows has no `deleted` column                (added by m008)
--   * workflow_runs has no token columns               (added by m007)

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
