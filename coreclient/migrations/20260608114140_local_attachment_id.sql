-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
PRAGMA defer_foreign_keys = ON;

CREATE TABLE attachment_new (
    local_attachment_id BLOB NOT NULL PRIMARY KEY,
    attachment_id BLOB UNIQUE,
    chat_id BLOB NOT NULL,
    message_id BLOB NOT NULL,
    content_type TEXT NOT NULL,
    content BLOB,
    status INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES message (message_id) ON DELETE CASCADE
);

INSERT INTO
    attachment_new
SELECT
    attachment_id,
    attachment_id,
    chat_id,
    message_id,
    content_type,
    content,
    status,
    created_at
FROM
    attachment;

-- for the range/IN query + FK cascade + load_ids_by_message_id
DROP TABLE attachment;

ALTER TABLE attachment_new
RENAME TO attachment;

-- Recreate the indexes
CREATE INDEX idx_attachment_chat_id ON attachment (chat_id);

CREATE INDEX idx_attachment_created_at ON attachment (created_at);

CREATE INDEX idx_attachment_pending_ordered ON attachment (created_at)
WHERE
    status = 1;

CREATE INDEX idx_attachment_message_id ON attachment (message_id);

-- chat message queue does not have to clean up attachments anymore
ALTER TABLE chat_message_queue
DROP COLUMN attachment_id;
