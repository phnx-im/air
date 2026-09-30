-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- A table to store pending connection requests.
-- All pending requests of one sender share a chat, the one of the newest
-- request. A request is identified by the chat id its connection group derives,
-- which is its chat's id while it is the newest.
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
    -- The group chat a request via a group went through, if it still exists
    origin_chat_id BLOB,
    FOREIGN KEY (chat_id) REFERENCES chat(chat_id) ON DELETE CASCADE,
    FOREIGN KEY (origin_chat_id) REFERENCES chat(chat_id) ON DELETE SET NULL
);

CREATE INDEX idx_pending_connection_request_chat_id ON pending_connection_request(chat_id);

INSERT INTO pending_connection_request(request_id, chat_id, created_at, received_at, connection_info, username, connection_offer_hash, connection_package_hash)
SELECT
    p.chat_id,
    p.chat_id,
    p.created_at,
    COALESCE((
        SELECT
            MIN(m.timestamp)
        FROM message m
        WHERE
            m.chat_id = p.chat_id), p.created_at), p.connection_info, p.handle, p.connection_offer_hash, p.connection_package_hash
FROM
    pending_connection_info p;

DROP TABLE pending_connection_info;

DELETE FROM username_contact
WHERE chat_id IN (
        SELECT
            chat_id
        FROM
            chat
        WHERE
            is_incoming = 1
            AND is_confirmed_connection = 0);

DELETE FROM targeted_message_contact
WHERE chat_id IN (
        SELECT
            chat_id
        FROM
            chat
        WHERE
            is_incoming = 1
            AND is_confirmed_connection = 0);

