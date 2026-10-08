BEGIN;
CREATE TABLE IF NOT EXISTS checkout_agent_sessions (
  payment_id uuid NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
  session_id text NOT NULL,
  requests integer NOT NULL DEFAULT 1,
  window_started_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY(payment_id,session_id)
);
COMMIT;
