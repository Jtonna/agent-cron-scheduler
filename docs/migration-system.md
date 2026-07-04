# Migration System

This document describes how the Agent Cron Scheduler (ACS) evolves its
on-disk SQLite schema across releases.

The system is split into two layers, like a package and its consumer:

* **[`milepost`](../milepost/README.md)** — a generic, application-agnostic
  SQLite migration framework. It is a separate crate at the repository root
  (a sibling of `acs/`), versioned independently, and knows nothing about
  ACS. See the crate's [README](../milepost/README.md) for the standalone
  framework overview, and run `cargo doc --open -p milepost` for the full
  rustdoc API reference.
* **ACS's own migrations** — the actual schema-change files that consume the
  framework, living in `acs/src/migrations/`. These know everything about
  ACS's schema and nothing about how the framework runs them.

This document covers both layers together: the framework's design (owned by
`milepost`) and how ACS registers and runs its migrations on top of it.

For where migration state fits into the rest of ACS's persistence model, see
[Storage](storage.md#9-migration-system).

---

## 1. Overview

**Sources:** `milepost/src/lib.rs` (framework),
`acs/src/migrations/mod.rs` (registry + runner configuration),
`acs/src/migrations/m*.rs`, `acs/src/migrations/shell_tokens.rs`

Since v5.0.0 the migration system is split like a package and its consumer:

* **`milepost/`** is a generic, reusable migration framework — a sibling
  crate consumed as a path dependency, versioned independently (0.1.0). It
  contains ONLY framework functionality (the `Migration` trait, the
  `MigrationTx` SQL-string API, the runner, tracking-table management) and
  knows nothing about ACS.
* **`acs/src/migrations/`** owns everything ACS-specific: the migration
  files, the registry, and the runner configuration — which database file
  to migrate and the schema probe (an ACS database is recognised by its
  `workflows` table).

The separation exists for maintenance, clarity, and future development: the
framework and the shipped migrations evolve, get reviewed, and get tested
independently, and nothing in the framework can reach into ACS's live model
types. The daemon calls `acs::migrations::run_pending(data_dir)` at startup
(on a blocking task), which configures a `milepost::Runner` and executes it
before the storage layer opens `acs.db`. Any migration error is fatal: the
daemon logs it and exits rather than running against a partially-migrated
database.

```rust
// acs/src/migrations/mod.rs — the entire ACS-side configuration:
Runner::new(data_dir.join("acs.db"))
    .migrations(registry())                    // ACS's Vec<Box<dyn Migration>>
    .schema_probe(|tx| tx.table_exists("workflows"))
    .run()
```

---

## 2. The milepost framework

**Every migration is a Rust file** (`acs/src/migrations/mNNN_<name>.rs`)
implementing the framework's `Migration` trait:

```rust
pub trait Migration: Send + Sync {
    fn name(&self) -> &'static str;      // stable; PK in schema_migrations
    fn baseline(&self) -> bool { false } // baseline convention hook
    fn rebuild(&self) -> bool { false }  // PRAGMA rebuild convention hook
    fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError>;
}
```

The framework hands `up()` a `MigrationTx` — a small SQL-string API over the
runner-owned transaction (no ORM, no derive machinery):

* `execute_batch(sql)` — run one or more `;`-separated statements from a SQL
  string constant. No `BEGIN`/`COMMIT`; the runner owns the transaction.
* `execute(sql, &[SqlValue])` — one statement with positional parameters;
  returns rows affected.
* `query(sql, &[SqlValue]) -> Vec<Vec<SqlValue>>` — read query output back
  as plain Rust values (`SqlValue`: `Null` / `Integer` / `Real` / `Text` /
  `Blob`, with `as_str()` / `as_i64()` / `as_f64()` / `as_blob()` accessors).

Simple migrations keep their SQL in a string constant and are a single
`execute_batch` call. Complex migrations mix SQL strings with Rust-level
logic — querying rows, transforming them in Rust (shell tokenization,
recursion through nested step JSON), and writing back via parameterised SQL
— in ways plain SQL cannot express (m005 and m006 do exactly this).

Migrations are **frozen by construction**: SQL constants never change after
they ship, and the Rust logic operates purely on `serde_json::Value` — never
on the live model structs — so changes to the runtime models can never
require editing an already-shipped migration.

---

## 3. ACS migrations

ACS's migration files live in `acs/src/migrations/`, one Rust file per
migration (`mNNN_<name>.rs`), each implementing the `milepost::Migration`
trait described above. The registry in `acs/src/migrations/mod.rs` lists
them in ascending name order; `run_pending(data_dir)` configures the
`milepost::Runner` with this registry, the `acs.db` path, and a schema probe
that recognises an ACS database by its `workflows` table (see the
`Runner::new` snippet in [§1](#1-overview)).

---

## 4. schema_migrations tracking

### Tracking table: `schema_migrations`

Migration execution is tracked by name in a `schema_migrations` table inside
`{data_dir}/acs.db`. The table is created by the **runner itself** — not by
a migration — before any migration logic runs:

```sql
CREATE TABLE schema_migrations (
    name        TEXT PRIMARY KEY,             -- stable migration name, e.g. "m008_add_workflow_deleted"
    applied_at  TEXT NOT NULL,                -- RFC 3339 timestamp of the recording
    status      TEXT NOT NULL CHECK (status IN ('success','failed')),
    duration_ms INTEGER,                      -- wall-clock run time; NULL for seeded rows
    error       TEXT                          -- error text for failed rows; NULL otherwise
);
```

Migration status is **not** exposed through `/health`. The table is the only
structured surface; per-migration outcomes are also written to `daemon.log`
via `tracing` lines (e.g. `migration 'm007_add_token_columns' applied in 3ms`
and the startup summary `Migrations applied: [...]`).

### Runner behaviour

`run_pending()` walks the registry in name order. The tracking table — and
only the tracking table — decides what executes:

| Row state for a migration | Action |
|---|---|
| No row | Run it inside its own transaction, then record `success` or `failed` |
| `status = 'success'` | Skip without executing |
| `status = 'failed'` | **Abort daemon startup** before anything runs, naming the migration and the exact recovery statement |

Failed rows are detected up front: if ANY registry migration has a `failed`
row, the runner records nothing and aborts immediately — even migrations
earlier in the order that have no row yet do not run.

Each migration executes inside its own transaction. On failure the
migration is **rolled back completely**, its row is recorded with status
`failed` plus the error text, and the runner aborts: later migrations never
run and get no row.

Rows recorded for names the registry does not know — `m001_jobs_to_workflows`
and `m002_json_to_sqlite` on databases upgraded from v4 — are tolerated: they
are reported at info level and left in place, never an error.

**Recovery workflow for a failed migration** (this is the sanctioned
workflow): fix the underlying issue, then delete the tracking row so the next
startup re-runs it:

```sql
DELETE FROM schema_migrations WHERE name = '<migration name>';
```

**Caution:** unlike the pre-v5 Rust migrations, v5 migrations are not
internally idempotent — `m004_drop_input_schema` re-run against a migrated
schema fails, for example. Delete a row (failed *or* success) only when the
migration's preconditions have been restored; the tracking table, not
in-migration guards, is what prevents double execution.

---

## 5. Conventions (baseline/rebuild)

### Baseline convention (fresh install vs. upgrade)

`m000_baseline` is the fresh-install starting point: it creates the
pre-m003-era schema (inline `UNIQUE` on `workflows.name`, `input_schema`
column present, no token columns, no `deleted` column) so that m003–m008
apply on top and reproduce exactly the schema history an upgraded database
went through.

The baseline opts in via the trait's `baseline()` hook, which changes one
thing: on a database that **already has both migration tracking and a
schema** (the `schema_migrations` table pre-exists and ACS's configured
schema probe finds the `workflows` table — true of every v4.2.14 install),
the runner records a `success` row for the baseline WITHOUT executing it.  The baseline therefore never runs against an
existing schema, and a v4.2.14 database upgrading to v5 executes
**nothing**: baseline seeded, m003–m008 skipped via their recorded rows.

If the tracking table exists but the schema does not — a previous startup
died after the runner created the tracking table but before the baseline
ever executed — the baseline runs normally instead of being seeded, so the
crash window cannot wedge the database.

A database that has a schema but **no** tracking table predates migration
tracking; the framework rejects it before creating or running anything,
with its default guidance ("… has a schema but no schema_migrations
tracking table. Refusing to run migrations against it: seed
schema_migrations rows for the migrations that are already applied, or
start from a database with recorded history."). A database that has a
schema and a tracking table with **zero recorded rows** (lost history) is
likewise rejected ("… has a schema and a schema_migrations table, but no
recorded migration history. Refusing to run migrations against it: …")
instead of seeding the baseline and then failing on migrations built for
the fresh-install chain.

### Rebuild convention (`PRAGMA foreign_keys`)

`PRAGMA foreign_keys` is a no-op inside a transaction, so a migration that
rebuilds a table participating in a foreign key (drop + rename of a parent
table) cannot toggle enforcement itself. A migration that opts in via the
trait's `rebuild()` hook gets this treatment from the runner instead:

1. `PRAGMA foreign_keys = OFF` before the transaction opens;
2. the migration body runs inside the transaction;
3. `PRAGMA foreign_key_check` runs inside the transaction before commit —
   any violation fails (and therefore rolls back) the migration;
4. `PRAGMA foreign_keys = ON` is restored after commit or rollback.

`m008_add_workflow_deleted` is the concrete case: it rebuilds `workflows`
(the parent of `workflow_runs.workflow_id`) to drop the inline `UNIQUE`.

---

## 6. Adding a migration

1. Create `acs/src/migrations/mNNN_<name>.rs` (increment NNN past the last
   entry) with a unit struct implementing `milepost::Migration`. Keep the
   SQL in string constants; the body is usually a single
   `tx.execute_batch(SQL)`. No `BEGIN`/`COMMIT` — the runner owns the
   transaction.
2. Declare the module and append `Box::new(mNNN_<name>::YourStruct)` to
   `registry()` in `acs/src/migrations/mod.rs` — names must stay in
   ascending order.
3. Add Rust-level logic (via `query` / `execute`) only where SQL alone
   cannot express the transform, documenting why.

---

## 7. Migration inventory & history

### m000_baseline (v5.0.0)

A single SQL constant; opts into the baseline convention (`baseline()`
returns `true`). Creates the pre-m003-era schema on fresh installs:
`workflows` (with inline `UNIQUE` on `name` and the `input_schema` column,
without `deleted`), `workflow_runs` (without the token columns), the three
`workflow_runs` indexes, and `meta`. Never executes on a database that
already has migration tracking (see the baseline convention above).

### m001_jobs_to_workflows / m002_json_to_sqlite (retired)

The v4-era Rust migrations that converted the pre-ACS-18 `jobs.json` format
to workflows and moved JSON storage into SQLite were retired in v5.0.0,
along with the `migrations.json` backfill. Databases that still need them
are not supported for direct upgrade (the framework rejects a schema
without recorded migration history). Their `schema_migrations` rows are
tolerated and left in place on upgraded databases.

### m003_drop_step_output_summary

A single SQL constant (json1). Strips the legacy `output_summary` key from
every persisted
`StepRun` record in `workflow_runs.steps_json` — per-step output lives in
`{data_dir}/logs/{workflow_id}/{run_id}.log`, framed by each `StepRun`'s
byte offsets, so the inline copy was redundant. The `UPDATE` rebuilds each
`steps_json` array element-by-element with `json_each` + `json_remove`,
consulting the `json_each.type` column so every element kind round-trips
exactly; only rows where at least one object element actually carries the
key are rewritten.

### m004_drop_input_schema

A single SQL constant: `ALTER TABLE workflows DROP COLUMN input_schema;` —
the column is no
longer carried on `Workflow` / `NewWorkflow` / `WorkflowUpdate`. The column
is guaranteed present when this runs: fresh installs get it from the
baseline, and upgraded databases skip via the recorded row.

### m005_shell_claude_to_agent

**SQL strings + Rust logic** — the rewrite needs a shell tokenizer
(double/single quotes, backslash escapes), `-p` prompt extraction across
three flag syntaxes, residual-flag template reconstruction, and recursion
into arbitrarily-nested `match` step branches, none of which SQL alone can
express. The body queries `workflows` through `MigrationTx`, transforms the
step JSON in Rust, and writes back via parameterised SQL.

Rewrites legacy `shell` steps that wrap a
`claude -p ... --output-format stream-json` invocation as proper `agent`
steps of type `claude_code_cli`, so the streaming NDJSON cost parser
captures `cost_usd`. A step is rewritten when `kind == "shell"`, the
command starts with the literal token `claude`, and it contains
`--output-format stream-json`. The prompt comes from the first `-p` flag
(`-p "..."`, `-p '...'`, or `-p=...`; escape sequences preserved verbatim).
If the residual flags exactly match the default `claude_code_cli` tail the
step inherits the default template; otherwise a full `command_template` is
emitted so custom flags survive. Commands matching the criterion without a
`-p` flag (stdin-fed prompt) are left unchanged with a warning. Rewritten
workflows get `version + 1` and a fresh `updated_at`.

### m006_agent_step_normalize

**SQL strings + Rust logic** — same rationale and shape as m005 (tokenizer +
nested-JSON recursion).

Normalizes `agent` steps that still carry a legacy `command_template` string
into the structured shape: `--model <value>` / `--model=<value>` becomes the
`model` field, the canonical baseline tokens (`claude`, `-p` + value,
`--output-format stream-json`, `--verbose`,
`--dangerously-skip-permissions`) are stripped, whatever remains becomes
`extra_args` (defaulting to `[]`), and `command_template` is removed.
Recurses into `match` branches. Templates whose first token is not
`claude`, or whose quoting is malformed, are left unchanged with a warning.

### m007_add_token_columns

A single SQL constant. Adds `total_input_tokens` / `total_output_tokens`
(`INTEGER NOT NULL DEFAULT 0`) to `workflow_runs`; historical rows backfill
to `0` ("tokens not tracked").

### m008_add_workflow_deleted

A SQL constant behind an already-applied guard (if the `deleted` column
already exists — the tracking row was lost — it fails loud with the exact
restore-the-row statement instead of silently resetting soft-delete state);
opts into the rebuild convention (`rebuild()` returns `true`, so the runner
applies the `PRAGMA foreign_keys` treatment above).
Adds the `deleted` soft-delete column to
`workflows` and replaces the inline `UNIQUE` on `name` — which SQLite cannot
drop in place — with a partial unique index over live rows:

```sql
CREATE UNIQUE INDEX idx_workflows_name_live ON workflows(name) WHERE deleted = 0;
```

The SQL creates `workflows_new` in the final shape, copies all rows
(`deleted` defaults to `0`), drops the old table, renames, and creates the
index; the runner's `foreign_key_check` guards against orphaning
`workflow_runs` rows. `DELETE /api/workflows/{id}` sets `deleted = 1`,
keeping the row (name stored verbatim) and every `workflow_runs` record so
cost/token history survives; the partial index makes the name immediately
reusable by a new live workflow.

---

## 8. Upgrade path

| Installed version | Path to v5 |
|---|---|
| Fresh install | Baseline + m003–m008 run in order; all recorded `success` |
| v4.2.14 | Direct.  The runner executes nothing: baseline seeded, m003–m008 skip via recorded rows, m001/m002 rows tolerated |
| Pre-tracking databases | Not supported for direct upgrade: the framework rejects a schema without recorded migration history before running anything |
