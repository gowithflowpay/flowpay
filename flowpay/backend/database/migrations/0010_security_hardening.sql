BEGIN;

ALTER TABLE approvals
  DROP CONSTRAINT IF EXISTS approvals_status_check;

ALTER TABLE approvals
  ADD CONSTRAINT approvals_status_check
  CHECK (status IN ('PENDING','APPROVED','EXECUTING','REJECTED','EXPIRED','CONSUMED'));

CREATE INDEX IF NOT EXISTS approvals_executable_idx
  ON approvals(status, expires_at)
  WHERE status IN ('APPROVED','EXECUTING');

COMMIT;
