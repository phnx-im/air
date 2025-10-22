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
    locked_by BLOB,
    PRIMARY KEY (group_id),
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);
