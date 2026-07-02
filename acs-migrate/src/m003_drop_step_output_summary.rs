//! m003_drop_step_output_summary — strip the legacy `output_summary` field
//! from every StepRun record persisted in workflow_runs.steps_json.
//!
//! Per-step output lives in the run's log file, framed by each StepRun's
//! log_byte_offset_start / log_byte_offset_end pair; the inline copy older
//! runs persisted is redundant.
//!
//! steps_json is a JSON array of StepRun objects. The UPDATE rebuilds the
//! array element by element, removing the top-level `output_summary` key
//! from object elements. The json_each `type` column is consulted so every
//! element kind round-trips exactly (objects/arrays re-parsed via json(),
//! JSON booleans/null re-emitted literally, numbers and strings passed
//! through as native SQL values). Only rows where at least one object
//! element actually carries the key are rewritten.

use crate::{MigrateError, Migration, MigrationTx};

const SQL: &str = r#"
UPDATE workflow_runs
SET steps_json = (
    SELECT json_group_array(
        CASE je.type
            WHEN 'object' THEN json(json_remove(je.value, '$.output_summary'))
            WHEN 'array'  THEN json(je.value)
            WHEN 'true'   THEN json('true')
            WHEN 'false'  THEN json('false')
            WHEN 'null'   THEN json('null')
            ELSE je.value
        END
    )
    FROM json_each(workflow_runs.steps_json) AS je
)
WHERE json_type(steps_json) = 'array'
  AND EXISTS (
      SELECT 1
      FROM json_each(workflow_runs.steps_json) AS je
      WHERE je.type = 'object'
        AND json_type(je.value, '$.output_summary') IS NOT NULL
  );
"#;

pub(crate) struct DropStepOutputSummary;

impl Migration for DropStepOutputSummary {
    fn name(&self) -> &'static str {
        "m003_drop_step_output_summary"
    }

    fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError> {
        tx.execute_batch(SQL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open");
        conn.execute_batch(
            "CREATE TABLE workflow_runs (run_id TEXT PRIMARY KEY, steps_json TEXT NOT NULL);",
        )
        .expect("schema");
        conn
    }

    fn run_migration(conn: &mut Connection) {
        let tx = conn.transaction().expect("tx");
        DropStepOutputSummary
            .up(&MigrationTx::new(&tx))
            .expect("migrate");
        tx.commit().expect("commit");
    }

    fn steps_json(conn: &Connection, run_id: &str) -> String {
        conn.query_row(
            "SELECT steps_json FROM workflow_runs WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .expect("read")
    }

    #[test]
    fn strips_output_summary_and_preserves_other_fields() {
        let mut conn = setup_conn();
        conn.execute(
            "INSERT INTO workflow_runs VALUES ('r1', ?1)",
            [r#"[{"step_id":"s0","output_summary":"noise","exit_code":0},{"step_id":"s1"}]"#],
        )
        .expect("insert");

        run_migration(&mut conn);

        let after: serde_json::Value = serde_json::from_str(&steps_json(&conn, "r1")).unwrap();
        assert!(after[0].get("output_summary").is_none());
        assert_eq!(after[0]["step_id"], "s0");
        assert_eq!(after[0]["exit_code"], 0);
        assert_eq!(after[1]["step_id"], "s1");
    }

    #[test]
    fn leaves_rows_without_the_key_untouched_byte_for_byte() {
        let mut conn = setup_conn();
        // Deliberately non-minified JSON: an untouched row must not be
        // rewritten (and therefore not re-minified).
        let original = r#"[ {"step_id": "s0", "exit_code": 0} ]"#;
        conn.execute("INSERT INTO workflow_runs VALUES ('r1', ?1)", [original])
            .expect("insert");

        run_migration(&mut conn);

        assert_eq!(
            steps_json(&conn, "r1"),
            original,
            "rows without output_summary must not be rewritten"
        );
    }
}
