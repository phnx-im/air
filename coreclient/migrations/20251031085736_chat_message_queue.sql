-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Chat Messages scheduled for being sent out.
CREATE TABLE chat_message_queue (
    message_id BLOB PRIMARY KEY NOT NULL,
    chat_id BLOB NOT NULL,
    -- Optional attachment id if the message has an attachment that needs to be
    -- cleaned up in case sending fails.
    attachment_id BLOB,
    created_at TEXT NOT NULL,
    locked_by BLOB,
    FOREIGN KEY (message_id) REFERENCES message (message_id) ON DELETE CASCADE
);

CREATE INDEX idx_chat_message_queue_created_at ON chat_message_queue (created_at);
