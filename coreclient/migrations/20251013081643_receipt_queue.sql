-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Message receipts scheduled for being sent out.
CREATE TABLE receipt_queue (
    message_id BLOB NOT NULL,
    chat_id BLOB NOT NULL,
    mimi_id BLOB NOT NULL,
    status INT NOT NULL,
    created_at TEXT NOT NULL,
    locked_by BLOB,
    locked_at TEXT,
    PRIMARY KEY (message_id, status),
    FOREIGN KEY (message_id) REFERENCES message (message_id) ON DELETE CASCADE
);

CREATE INDEX idx_receipt_queue_created_at ON receipt_queue (created_at);
