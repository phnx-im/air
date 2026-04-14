-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Privacy Pass tokens

CREATE TABLE privacy_pass_token_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_type INTEGER NOT NULL,
    token BLOB NOT NULL
);

INSERT INTO privacy_pass_token_new (id, operation_type, token)
SELECT id, 1, token FROM privacy_pass_token;

DROP TABLE privacy_pass_token;
ALTER TABLE privacy_pass_token_new RENAME TO privacy_pass_token;

-- Batched token keys

DROP TABLE batched_token_key;
CREATE TABLE batched_token_key (
    token_key_id INTEGER PRIMARY KEY,
    operation_type INTEGER NOT NULL,
    public_key BLOB NOT NULL
);
