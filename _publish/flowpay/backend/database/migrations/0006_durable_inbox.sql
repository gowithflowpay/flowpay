ALTER TABLE processed_messages
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'COMPLETED',
    ADD COLUMN IF NOT EXISTS received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN IF NOT EXISTS processing_started_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS failed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS attempt_count INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN IF NOT EXISTS last_error TEXT;

UPDATE processed_messages
SET status = 'COMPLETED',
    completed_at = COALESCE(completed_at, processed_at)
WHERE completed_at IS NULL;

ALTER TABLE processed_messages
    DROP CONSTRAINT IF EXISTS processed_messages_status_check;

ALTER TABLE processed_messages
    ADD CONSTRAINT processed_messages_status_check
        CHECK (status IN ('PROCESSING', 'COMPLETED', 'FAILED')),
    DROP CONSTRAINT IF EXISTS processed_messages_attempt_count_check;

ALTER TABLE processed_messages
    ADD CONSTRAINT processed_messages_attempt_count_check
        CHECK (attempt_count > 0);

CREATE INDEX IF NOT EXISTS idx_processed_messages_recoverable
    ON processed_messages (next_attempt_at, processing_started_at)
    WHERE status IN ('PROCESSING', 'FAILED');
