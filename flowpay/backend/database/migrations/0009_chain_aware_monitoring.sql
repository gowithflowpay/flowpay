BEGIN;

ALTER TABLE payment_monitor_cursors
  DROP CONSTRAINT IF EXISTS payment_monitor_cursors_pkey;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conname = 'payment_monitor_cursors_pkey'
      AND conrelid = 'payment_monitor_cursors'::regclass
  ) THEN
    ALTER TABLE payment_monitor_cursors
      ADD CONSTRAINT payment_monitor_cursors_pkey PRIMARY KEY (payment_id, chain);
  END IF;
END $$;

ALTER TABLE payments
  ALTER COLUMN overpayment_policy SET DEFAULT 'REQUIRE_REVIEW';

COMMIT;
