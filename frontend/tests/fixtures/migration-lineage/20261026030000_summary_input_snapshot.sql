-- Inputs for the displayed result and the in-flight run have separate lifetimes.
-- A failed/cancelled regeneration must retain the old result's provenance.
ALTER TABLE summary_processes ADD COLUMN input_snapshot TEXT;
ALTER TABLE summary_processes ADD COLUMN pending_input_snapshot TEXT;
