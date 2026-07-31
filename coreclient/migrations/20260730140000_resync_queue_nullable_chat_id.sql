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
ALTER TABLE resync_queue RENAME TO resync_queue_old;

CREATE TABLE resync_queue (
    group_id BLOB NOT NULL,
    pq_group_id BLOB,
    chat_id BLOB UNIQUE,
    group_state_ear_key BLOB NOT NULL,
    identity_link_wrapper_key BLOB NOT NULL,
    original_leaf_index INTEGER NOT NULL,
    vc_epoch_id BLOB,
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
    vc_epoch_id,
    locked_by
)
SELECT
    group_id,
    pq_group_id,
    chat_id,
    group_state_ear_key,
    identity_link_wrapper_key,
    original_leaf_index,
    vc_epoch_id,
    locked_by
FROM resync_queue_old;

DROP TABLE resync_queue_old;
