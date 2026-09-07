-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Record the key each cached token was issued under, so a key rotation only
-- discards the tokens of keys the AS has stopped advertising.
--
-- Existing rows carry no key ID. Where an operation type has exactly one
-- cached key, that key issued its tokens. Otherwise the issuer cannot be
-- recovered, and keeping a token that is already dead costs the user an
-- allowance slot, so those rows are dropped by the join below.
CREATE TABLE privacy_pass_token_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_type INTEGER NOT NULL,
    token BLOB NOT NULL,
    token_key_id INTEGER NOT NULL,
    created_at DATETIME NOT NULL
);

INSERT INTO privacy_pass_token_new (
    id,
    operation_type,
    token,
    token_key_id,
    created_at
)
SELECT
    token.id,
    token.operation_type,
    token.token,
    single_key.token_key_id,
    token.created_at
FROM privacy_pass_token AS token
JOIN (
    SELECT
        operation_type,
        MIN(token_key_id) AS token_key_id
    FROM batched_token_key
    GROUP BY operation_type
    HAVING COUNT(*) = 1
) AS single_key ON single_key.operation_type = token.operation_type;

DROP TABLE privacy_pass_token;

ALTER TABLE privacy_pass_token_new
RENAME TO privacy_pass_token;
