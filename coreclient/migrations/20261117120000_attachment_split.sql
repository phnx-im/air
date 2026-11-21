-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Splits attachments into content, remote location, transfer and message references.
CREATE TABLE message_attachment (
    message_id BLOB NOT NULL,
    part_index INTEGER NOT NULL,
    attachment_id BLOB NOT NULL,
    role INTEGER NOT NULL, -- 1 attachment, 2 preview
    filename TEXT,
    PRIMARY KEY (message_id, part_index),
    FOREIGN KEY (message_id) REFERENCES message (message_id) ON DELETE CASCADE,
    FOREIGN KEY (attachment_id) REFERENCES attachment (attachment_id) ON DELETE CASCADE
);

CREATE INDEX idx_message_attachment_attachment_id ON message_attachment (attachment_id);

CREATE TABLE attachment_remote (
    attachment_id BLOB NOT NULL PRIMARY KEY,
    remote_attachment_id BLOB NOT NULL UNIQUE,
    size INTEGER NOT NULL,
    enc_alg INTEGER NOT NULL,
    enc_key BLOB NOT NULL,
    nonce BLOB NOT NULL,
    aad BLOB NOT NULL,
    hash_alg INTEGER NOT NULL,
    hash BLOB NOT NULL,
    FOREIGN KEY (attachment_id) REFERENCES attachment (attachment_id) ON DELETE CASCADE
);

CREATE TABLE attachment_transfer (
    attachment_id BLOB NOT NULL PRIMARY KEY,
    direction INTEGER NOT NULL, -- 1 download, 2 upload
    state INTEGER NOT NULL, -- 1 queued, 2 running, 3 failed
    failure INTEGER, -- set iff failed: 1 unknown, 2 not found
    started_at TEXT,
    FOREIGN KEY (attachment_id) REFERENCES attachment (attachment_id) ON DELETE CASCADE
);

ALTER TABLE attachment
ADD COLUMN size INTEGER;

ALTER TABLE attachment
ADD COLUMN content_hash BLOB;

ALTER TABLE attachment
ADD COLUMN width INTEGER;

ALTER TABLE attachment
ADD COLUMN height INTEGER;

ALTER TABLE attachment
ADD COLUMN blurhash TEXT;
