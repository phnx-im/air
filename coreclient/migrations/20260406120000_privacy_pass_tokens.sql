-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

CREATE TABLE privacy_pass_token (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    token BLOB NOT NULL
);

CREATE TABLE batched_token_key (
    token_key_id INTEGER PRIMARY KEY,
    public_key BLOB NOT NULL
);
