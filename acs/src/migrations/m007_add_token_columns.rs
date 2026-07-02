//! m007_add_token_columns — add total_input_tokens / total_output_tokens to
//! workflow_runs.
//!
//! Token counts became first-class alongside USD cost: each completed run
//! records how many input and output tokens the agent consumed, summed
//! across all agent steps. Historical rows carry 0 for both fields, the
//! sentinel for "tokens not tracked".

use milepost::{MigrateError, Migration, MigrationTx};

const SQL: &str = "
ALTER TABLE workflow_runs ADD COLUMN total_input_tokens  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE workflow_runs ADD COLUMN total_output_tokens INTEGER NOT NULL DEFAULT 0;
";

pub(crate) struct AddTokenColumns;

impl Migration for AddTokenColumns {
    fn name(&self) -> &'static str {
        "m007_add_token_columns"
    }

    fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError> {
        tx.execute_batch(SQL)
    }
}
