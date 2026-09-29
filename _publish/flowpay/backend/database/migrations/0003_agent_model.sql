-- Model provenance for auditable tool-using investigations.
ALTER TABLE agent_runs
    ADD COLUMN IF NOT EXISTS model_provider text,
    ADD COLUMN IF NOT EXISTS model_name text,
    ADD COLUMN IF NOT EXISTS orchestration_mode text NOT NULL DEFAULT 'DETERMINISTIC'
        CHECK (orchestration_mode IN ('MODEL_TOOL_USE','DETERMINISTIC'));

CREATE INDEX IF NOT EXISTS agent_runs_mode_idx
    ON agent_runs(orchestration_mode, started_at DESC);
