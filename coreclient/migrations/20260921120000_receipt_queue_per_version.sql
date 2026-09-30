-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Queue one receipt per Mimi ID instead of per message because an edit keeps
-- the message id but changes the Mimi ID.
CREATE TABLE receipt_queue_new(
    message_id BLOB NOT NULL,
    chat_id BLOB NOT NULL,
    mimi_id BLOB NOT NULL,
    status int NOT NULL,
    created_at text NOT NULL,
    locked_by BLOB,
    locked_at text,
    PRIMARY KEY (mimi_id, status),
    FOREIGN KEY (message_id) REFERENCES message(message_id) ON DELETE CASCADE
);

INSERT
    OR IGNORE INTO receipt_queue_new(message_id, chat_id, mimi_id, status, created_at, locked_by, locked_at)
    SELECT
        message_id,
        chat_id,
        mimi_id,
        status,
        created_at,
        locked_by,
        locked_at
    FROM
        receipt_queue;

DROP TABLE receipt_queue;

ALTER TABLE receipt_queue_new RENAME TO receipt_queue;

CREATE INDEX idx_receipt_queue_created_at ON receipt_queue(created_at);

CREATE INDEX idx_receipt_queue_locked_by ON receipt_queue(locked_by)
WHERE
    locked_by IS NOT NULL;

