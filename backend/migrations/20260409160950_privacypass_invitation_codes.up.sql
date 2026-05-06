-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Drop privacypass columns on as_client_record and create a table
-- that let us categorise token quotas by operation type
ALTER TABLE as_client_record
    DROP COLUMN remaining_tokens,
    DROP COLUMN allowance_epoch;

CREATE TABLE as_token_allowance(
    user_uuid uuid NOT NULL,
    user_domain TEXT NOT NULL,
    operation_type SMALLINT NOT NULL,
    remaining SMALLINT NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_uuid, user_domain, operation_type),
    FOREIGN KEY (user_uuid, user_domain) REFERENCES as_user_record (user_uuid, user_domain) ON DELETE CASCADE
);

-- Add operation_type to primary key of as_token_nonce
CREATE UNLOGGED TABLE as_token_nonce_new (
    operation_type SMALLINT NOT NULL,
    nonce BYTEA NOT NULL,
    status nonce_status NOT NULL DEFAULT 'reserved',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (operation_type, nonce)
);

INSERT INTO as_token_nonce_new (operation_type, nonce, status, created_at)
SELECT 1, nonce, status, created_at FROM as_token_nonce;

DROP TABLE as_token_nonce;
ALTER TABLE as_token_nonce_new RENAME TO as_token_nonce;

-- Add operation_type to primary key of as_batched_key
CREATE TABLE as_batched_key_new (
    operation_type SMALLINT NOT NULL,
    token_key_id SMALLINT NOT NULL,
    voprf_server BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (operation_type, token_key_id)
);

INSERT INTO as_batched_key_new (operation_type, token_key_id, voprf_server)
SELECT 1, token_key_id, voprf_server FROM as_batched_key;

DROP TABLE as_batched_key;
ALTER TABLE as_batched_key_new RENAME TO as_batched_key;

-- Add created_at column in invitation_code table
ALTER TABLE invitation_code
ADD COLUMN created_at timestamptz NOT NULL DEFAULT now ();

CREATE INDEX idx_invitation_code_created_at ON invitation_code (created_at DESC);
