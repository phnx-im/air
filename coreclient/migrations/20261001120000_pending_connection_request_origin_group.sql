-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Replace `origin_chat_id` with the group id of the group chat a request via a
-- group went through.
--
-- A device that got the request from a sibling may not have the group chat yet,
-- and the group chat may be deleted. So `origin_group_id` has no foreign key.
-- Existing requests take it from their origin chat, if it still exists.
ALTER TABLE pending_connection_request RENAME TO pending_connection_request_old;

CREATE TABLE pending_connection_request(
    request_id BLOB NOT NULL PRIMARY KEY,
    chat_id BLOB NOT NULL,
    created_at text NOT NULL,
    -- When the request was sent, as the server saw it
    received_at text NOT NULL,
    connection_info BLOB NOT NULL,
    -- The username a request via a username went to
    username text,
    connection_offer_hash BLOB,
    connection_package_hash BLOB,
    -- The group a request via a group went through
    origin_group_id BLOB,
    FOREIGN KEY (chat_id) REFERENCES chat(chat_id) ON DELETE CASCADE
);

INSERT INTO pending_connection_request(request_id, chat_id, created_at, received_at, connection_info, username, connection_offer_hash, connection_package_hash, origin_group_id)
SELECT
    r.request_id,
    r.chat_id,
    r.created_at,
    r.received_at,
    r.connection_info,
    r.username,
    r.connection_offer_hash,
    r.connection_package_hash,
    c.group_id
FROM
    pending_connection_request_old r
    LEFT JOIN chat c ON c.chat_id = r.origin_chat_id;

DROP TABLE pending_connection_request_old;

CREATE INDEX idx_pending_connection_request_chat_id ON pending_connection_request(chat_id);
