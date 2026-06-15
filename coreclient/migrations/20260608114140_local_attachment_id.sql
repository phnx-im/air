-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
PRAGMA defer_foreign_keys = ON;

-- Decouple the attachment id from the server-assigned id
CREATE TABLE attachment_new (
    attachment_id BLOB NOT NULL PRIMARY KEY,
    remote_attachment_id BLOB UNIQUE,
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

DROP TABLE attachment;

ALTER TABLE attachment_new
RENAME TO attachment;

CREATE INDEX idx_attachment_chat_id ON attachment (chat_id);

CREATE INDEX idx_attachment_created_at ON attachment (created_at);

CREATE INDEX idx_attachment_pending_ordered ON attachment (created_at)
WHERE
    status = 1;

CREATE INDEX idx_attachment_message_id ON attachment (message_id);

-- The pending attachment is keyed by the server-assigned id
-- The table must be recreated to move the foreign key constraint
CREATE TABLE pending_attachment_new (
    remote_attachment_id BLOB NOT NULL PRIMARY KEY,
    size INTEGER NOT NULL,
    enc_alg INTEGER NOT NULL,
    enc_key BLOB NOT NULL,
    nonce BLOB NOT NULL,
    aad BLOB NOT NULL,
    hash_alg INTEGER NOT NULL,
    hash BLOB NOT NULL,
    FOREIGN KEY (remote_attachment_id) REFERENCES attachment (remote_attachment_id) ON DELETE CASCADE
);

INSERT INTO
    pending_attachment_new
SELECT
    *
FROM
    pending_attachment;

DROP TABLE pending_attachment;

ALTER TABLE pending_attachment_new
RENAME TO pending_attachment;

-- chat message queue does not have to clean up attachments anymore
ALTER TABLE chat_message_queue
DROP COLUMN attachment_id;
