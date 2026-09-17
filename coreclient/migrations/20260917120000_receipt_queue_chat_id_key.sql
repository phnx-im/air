-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The same message status is now queued under more than one chat: the chat
-- itself and, to synchronize the read marker with our own devices, the self
-- chat.
ALTER TABLE receipt_queue RENAME TO receipt_queue_old;

CREATE TABLE receipt_queue (
    message_id BLOB NOT NULL,
    chat_id BLOB NOT NULL,
    mimi_id BLOB NOT NULL,
    status INT NOT NULL,
    created_at TEXT NOT NULL,
    locked_by BLOB,
    locked_at TEXT,
    PRIMARY KEY (message_id, chat_id, status),
    FOREIGN KEY (message_id) REFERENCES message (message_id) ON DELETE CASCADE
);

INSERT INTO receipt_queue
    (message_id, chat_id, mimi_id, status, created_at, locked_by, locked_at)
SELECT message_id, chat_id, mimi_id, status, created_at, locked_by, locked_at
FROM receipt_queue_old;

DROP TABLE receipt_queue_old;

CREATE INDEX idx_receipt_queue_created_at ON receipt_queue (created_at);
CREATE INDEX idx_receipt_queue_locked_by ON receipt_queue (locked_by);
