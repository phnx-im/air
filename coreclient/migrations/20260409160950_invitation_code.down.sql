-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Revert privacy_pass_token to original schema
CREATE TABLE privacy_pass_token_old (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    token BLOB NOT NULL
);

INSERT INTO
    privacy_pass_token_old (id, token)
SELECT
    id,
    token
FROM
    privacy_pass_token;

DROP TABLE privacy_pass_token;

ALTER TABLE privacy_pass_token_old
RENAME TO privacy_pass_token;

-- Revert batched_token_key to original schema (data is ephemeral, can be discarded)
DROP TABLE batched_token_key;

CREATE TABLE batched_token_key (
    token_key_id INTEGER PRIMARY KEY,
    public_key BLOB NOT NULL
);

DROP TABLE invitation_code;
