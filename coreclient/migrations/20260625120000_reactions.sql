-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Local state of emoji reactions (both our own and received ones).
--
-- A reaction is a MIMI message (disposition "reaction") whose `in_reply_to`
-- points at the reacted-to message. We don't store reactions as rows in the
-- `message` table (they are not displayed as tiles).
CREATE TABLE reaction (
    reaction_mimi_id BLOB NOT NULL PRIMARY KEY,
    target_mimi_id BLOB NOT NULL,
    chat_id BLOB NOT NULL,
    sender_user_uuid BLOB NOT NULL,
    sender_user_domain TEXT NOT NULL,
    emoji TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);

-- A user reacts to a given message at most once per emoji; reacting again with
-- the same emoji is idempotent, different emojis are independent reactions.
CREATE UNIQUE INDEX idx_reaction_unique ON reaction (
    target_mimi_id,
    sender_user_uuid,
    sender_user_domain,
    emoji
);

CREATE INDEX idx_reaction_target ON reaction (target_mimi_id);

-- Outgoing reaction MLS messages scheduled for being sent out.
--
-- Unlike the chat message queue, this queue carries the exact serialized
-- `MimiContent` to send, so that both adding a reaction and retracting one
-- (which deletes the `reaction` row) flow through a single send loop.
CREATE TABLE reaction_queue (
    id BLOB NOT NULL PRIMARY KEY,
    chat_id BLOB NOT NULL,
    -- The reaction row to roll back if sending fails permanently.
    -- NULL for retraction tombstones (the row is already gone).
    reaction_mimi_id BLOB,
    content BLOB NOT NULL,
    created_at TEXT NOT NULL,
    locked_by BLOB,
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);

CREATE INDEX idx_reaction_queue_created_at ON reaction_queue (created_at);
