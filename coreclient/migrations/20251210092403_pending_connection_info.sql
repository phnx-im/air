-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
ALTER TABLE chat
ADD COLUMN is_incoming BOOLEAN NOT NULL DEFAULT FALSE;

-- Information needed for accepting a pending connection request
CREATE TABLE pending_connection_info (
    chat_id BLOB NOT NULL PRIMARY KEY,
    created_at TEXT NOT NULL,
    connection_info BLOB NOT NULL,
    handle TEXT, -- User handle invitation
    connection_offer_hash BLOB, -- Only present for user handle invitation
    connection_package_hash BLOB, -- Only present for user handle invitation
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);
