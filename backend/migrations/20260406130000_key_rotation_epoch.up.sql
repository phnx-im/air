-- Add creation timestamp to VOPRF keys for rotation tracking
ALTER TABLE as_batched_key ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- Track which epoch (key) the user's allowance was last set against
ALTER TABLE as_client_record ADD COLUMN allowance_epoch SMALLINT;

-- Reset all existing allowances to the new per-epoch default
UPDATE as_client_record SET
    remaining_tokens = 10,
    allowance_epoch = COALESCE(
        (SELECT token_key_id FROM as_batched_key ORDER BY token_key_id DESC LIMIT 1),
        0
    );

-- Make allowance_epoch NOT NULL after backfill
ALTER TABLE as_client_record ALTER COLUMN allowance_epoch SET NOT NULL;
