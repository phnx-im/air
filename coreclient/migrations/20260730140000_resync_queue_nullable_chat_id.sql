-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Make `chat_id` nullable so a resync can be queued before its chat exists.
--
-- A resync that onboards an emulator client into a higher-level group has no
-- local chat yet: the chat is created from the group's `GroupData` once the
-- external commit succeeds, so a failed onboarding does not leave a chat behind
-- that is bound to no MLS group. Ordinary resyncs still carry their chat id, and
-- keep the cascade -- SQLite does not enforce a foreign key on NULL.
PRAGMA defer_foreign_keys = ON;

ALTER TABLE resync_queue RENAME TO resync_queue_old;

CREATE TABLE resync_queue (
    group_id BLOB NOT NULL,
    pq_group_id BLOB,
    chat_id BLOB UNIQUE,
    group_state_ear_key BLOB NOT NULL,
    identity_link_wrapper_key BLOB NOT NULL,
    original_leaf_index INTEGER NOT NULL,
    shares_vc_leaf BOOLEAN NOT NULL DEFAULT FALSE,
    locked_by BLOB,
    PRIMARY KEY (group_id),
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);

INSERT INTO resync_queue (
    group_id,
    pq_group_id,
    chat_id,
    group_state_ear_key,
    identity_link_wrapper_key,
    original_leaf_index,
    shares_vc_leaf,
    locked_by
)
SELECT
    group_id,
    pq_group_id,
    chat_id,
    group_state_ear_key,
    identity_link_wrapper_key,
    original_leaf_index,
    shares_vc_leaf,
    locked_by
FROM resync_queue_old;

DROP TABLE resync_queue_old;

-- Make `chat.is_active` nullable so `ChatStatus::Pending` can be represented as `NULL`.
CREATE TABLE chat_new (
    chat_id BLOB NOT NULL PRIMARY KEY,
    chat_title TEXT NOT NULL,
    chat_picture BLOB,
    group_id BLOB NOT NULL,
    last_read TEXT NOT NULL,
    -- missing `connection_as_{client_uuid,domain}` fields means it is a group chat
    connection_user_uuid BLOB,
    connection_user_domain TEXT,
    is_confirmed_connection BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    connection_user_handle TEXT,
    is_incoming BOOLEAN NOT NULL DEFAULT FALSE,
    muted_until DATETIME,
    notified_until DATETIME
);

INSERT INTO chat_new (
    chat_id,
    chat_title,
    chat_picture,
    group_id,
    last_read,
    connection_user_uuid,
    connection_user_domain,
    is_confirmed_connection,
    is_active,
    connection_user_handle,
    is_incoming,
    muted_until,
    notified_until
)
SELECT
    chat_id,
    chat_title,
    chat_picture,
    group_id,
    last_read,
    connection_user_uuid,
    connection_user_domain,
    is_confirmed_connection,
    is_active,
    connection_user_handle,
    is_incoming,
    muted_until,
    notified_until
FROM chat;

DROP TABLE chat;

ALTER TABLE chat_new RENAME TO chat;

CREATE INDEX idx_chat_connection_user ON chat (connection_user_uuid, connection_user_domain);
