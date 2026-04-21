-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Add operation type and created_at to privacy pass tokens table
CREATE TABLE privacy_pass_token_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_type INTEGER NOT NULL,
    token BLOB NOT NULL,
    created_at DATETIME NOT NULL
);

INSERT INTO
    privacy_pass_token_new (id, operation_type, token, created_at)
SELECT
    id,
    1, -- AddUsername
    token,
    CURRENT_TIMESTAMP
FROM
    privacy_pass_token;

DROP TABLE privacy_pass_token;

ALTER TABLE privacy_pass_token_new
RENAME TO privacy_pass_token;

-- Add operation type to batched token keys table
-- The data in this table can be removed
DROP TABLE batched_token_key;

CREATE TABLE batched_token_key (
    operation_type INTEGER NOT NULL,
    token_key_id INTEGER NOT NULL,
    public_key BLOB NOT NULL,
    PRIMARY KEY (operation_type, token_key_id)
);

-- Locally stored invitation codes
CREATE TABLE invitation_code (
    code TEXT NOT NULL PRIMARY KEY,
    copied BOOLEAN NOT NULL DEFAULT FALSE,
    created_at DATETIME NOT NULL
);
