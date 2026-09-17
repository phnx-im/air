-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Track the lifecycle of a queued resync.
DROP TABLE resync_queue;

CREATE TABLE resync_queue (
    group_id BLOB NOT NULL,
    pq_group_id BLOB,
    chat_id BLOB UNIQUE,
    group_state_ear_key BLOB NOT NULL,
    identity_link_wrapper_key BLOB NOT NULL,
    original_leaf_index INTEGER NOT NULL,
    shares_vc_leaf BOOLEAN NOT NULL DEFAULT FALSE,
    connection_contact BLOB,
    locked_by BLOB,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'failed')),
    reason TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    not_before TEXT,
    last_error TEXT,
    PRIMARY KEY (group_id),
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);
