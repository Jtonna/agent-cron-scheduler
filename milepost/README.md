# milepost

A small, table-driven SQLite migration framework. Like mileposts along a
road, migrations are named markers laid down in order — the framework
records which ones a database has passed and runs the rest.

`milepost` is a generic library: it contains only framework functionality
and knows nothing about the application using it. The application defines
its own migrations (Rust types implementing the `Migration` trait, with SQL
kept in string constants), builds a registry, and hands both to the
`Runner`.

## What it does

- **One migration kind**: every migration is a Rust type. Simple migrations
  are a single SQL string executed as a batch; complex migrations mix SQL
  strings with Rust-level logic through the `MigrationTx` API
  (`execute_batch` / `execute` / `query` over plain `SqlValue` rows).
- **Table-driven execution**: a `schema_migrations` table inside the target
  database is the sole authority — no row means run, a `success` row means
  skip, a `failed` row blocks every run until an operator deletes it (the
  error carries the exact recovery statement).
- **Per-migration transactions**: each migration commits or rolls back
  atomically; a failure records a `failed` row with the error text and
  stops the run.
- **Baseline convention**: a migration whose `baseline()` hook returns true
  creates the fresh-install schema, and is recorded without executing on
  databases that already have both tracking and a schema (detected by a
  caller-supplied probe).
- **Rebuild convention**: a migration whose `rebuild()` hook returns true
  gets `PRAGMA foreign_keys = OFF` around its transaction and a pre-commit
  `PRAGMA foreign_key_check`.
- **Tolerant tracking**: rows recorded for migration names the registry no
  longer ships are reported, never an error.

## Usage

```rust
use milepost::{MigrateError, Migration, MigrationTx, Runner};

struct CreateUsers;

impl Migration for CreateUsers {
    fn name(&self) -> &'static str {
        "m001_create_users"
    }
    fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError> {
        tx.execute_batch("CREATE TABLE users (id TEXT PRIMARY KEY);")
    }
}

fn migrate(db_path: &str) -> Result<(), MigrateError> {
    // The parent directory of `db_path` must already exist.
    let report = Runner::new(db_path)
        .migrations(vec![Box::new(CreateUsers)])
        .run()?;
    println!("applied: {:?}", report.ran);
    Ok(())
}
```

See the crate-level documentation for the full semantics, including the
schema probe used to guard databases that predate migration tracking.

This crate is developed and versioned independently of the application that
consumes it.
