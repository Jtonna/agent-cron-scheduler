-- m003_drop_step_output_summary — strip the legacy `output_summary` field
-- from every StepRun record persisted in workflow_runs.steps_json.
--
-- Per-step output lives in the run's log file, framed by each StepRun's
-- log_byte_offset_start / log_byte_offset_end pair; the inline copy older
-- runs persisted is redundant.
--
-- steps_json is a JSON array of StepRun objects. This UPDATE rebuilds the
-- array element by element, removing the top-level `output_summary` key from
-- object elements. The json_each `type` column is consulted so every element
-- kind round-trips exactly (objects/arrays re-parsed via json(), JSON
-- booleans/null re-emitted literally, numbers and strings passed through as
-- native SQL values). Only rows where at least one object element actually
-- carries the key are rewritten.

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
