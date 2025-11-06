-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Resync operation scheduled for being sent out.
CREATE TABLE resync_queue (
    group_id BLOB NOT NULL,
    chat_id BLOB UNIQUE NOT NULL,
    group_state_ear_key BLOB NOT NULL,
    identity_link_wrapper_key BLOB NOT NULL,
    original_leaf_index INTEGER NOT NULL,
    locked_by BLOB,
    PRIMARY KEY (group_id),
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);

-- Add missing foreign key constraint to group_membership table
ALTER TABLE group_membership RENAME TO group_membership_old;

CREATE TABLE group_membership (
    group_id BLOB NOT NULL,
    leaf_index INTEGER NOT NULL,
    status TEXT DEFAULT 'staged_update' NOT NULL CHECK (
        status IN (
            'staged_update',
            'staged_removal',
            'staged_add',
            'merged'
        )
    ),
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    PRIMARY KEY (group_id, leaf_index, status)
    FOREIGN KEY (group_id) REFERENCES "group" (group_id) ON DELETE CASCADE
);

INSERT INTO group_membership (group_id, leaf_index, status, user_uuid, user_domain)
SELECT group_id, leaf_index, status, user_uuid, user_domain
FROM group_membership_old;

DROP TABLE group_membership_old;
