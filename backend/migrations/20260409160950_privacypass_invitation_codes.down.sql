-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Restore privacypass columns on as_client_record and drop the operation-type table
DROP TABLE as_token_allowance;

ALTER TABLE as_client_record
ADD COLUMN remaining_tokens INTEGER NOT NULL DEFAULT 10,
ADD COLUMN allowance_epoch SMALLINT NOT NULL DEFAULT 0;

-- Recreate as_batched_key without operation_type in primary key
CREATE TABLE as_batched_key_old (
    token_key_id SMALLINT NOT NULL,
    voprf_server BYTEA NOT NULL,
    PRIMARY KEY (token_key_id)
);

INSERT INTO as_batched_key_old (token_key_id, voprf_server)
SELECT DISTINCT ON (token_key_id) token_key_id, voprf_server FROM as_batched_key;

DROP TABLE as_batched_key;
ALTER TABLE as_batched_key_old RENAME TO as_batched_key;

-- Recreate as_token_nonce without operation_type in primary key
CREATE UNLOGGED TABLE as_token_nonce_old (
    nonce BYTEA NOT NULL,
    status nonce_status NOT NULL DEFAULT 'reserved',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (nonce)
);

INSERT INTO as_token_nonce_old (nonce, status, created_at)
SELECT DISTINCT ON (nonce) nonce, status, created_at FROM as_token_nonce;

DROP TABLE as_token_nonce;
ALTER TABLE as_token_nonce_old RENAME TO as_token_nonce;

-- Drop index and created_at column from invitation_code
DROP INDEX IF EXISTS idx_invitation_code_created_at;

ALTER TABLE invitation_code
DROP COLUMN created_at;
