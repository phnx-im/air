-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Create the pending_chat_operation table, which contains pending chat
-- operations, where a pending operation corresponds to a commit or SelfRemove
-- proposal
CREATE TABLE pending_chat_operation (
    group_id BLOB NOT NULL PRIMARY KEY,
    operation_type TEXT NOT NULL CHECK (operation_type IN ('leave', 'delete', 'other')),
    operation_data BLOB NOT NULL,
    last_attempt TEXT,
    number_of_attempts INTEGER NOT NULL DEFAULT 0,
    locked_by BLOB,
    request_status TEXT NOT NULL CHECK (request_status IN ('waiting_for_queue_response', 'ready_to_retry')),
    FOREIGN KEY (group_id) REFERENCES "group" (group_id) ON DELETE CASCADE
);

