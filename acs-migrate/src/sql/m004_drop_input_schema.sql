-- m004_drop_input_schema — drop the `input_schema` column from `workflows`.
--
-- input_schema is no longer carried on the Workflow / NewWorkflow /
-- WorkflowUpdate structs and is not consumed by the runtime.
--
-- The column is guaranteed present here: on a fresh install the baseline
-- creates it, and on upgraded databases this migration is skipped via its
-- recorded schema_migrations row.

ALTER TABLE workflows DROP COLUMN input_schema;
