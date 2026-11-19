-- no-transaction
-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Drops the attachment columns the previous migration moved out.
--
-- Not atomic with its bookkeeping, so it must be safe to run twice.
PRAGMA foreign_keys = OFF;

BEGIN;

DROP TABLE IF EXISTS attachment_new;

CREATE TABLE attachment_new (
    attachment_id BLOB NOT NULL PRIMARY KEY,
    content_type TEXT NOT NULL,
    size INTEGER,
    content_hash BLOB,
    width INTEGER,
    height INTEGER,
    blurhash TEXT,
    is_animated BOOLEAN,
    created_at TEXT NOT NULL
);

INSERT INTO
    attachment_new (
        attachment_id,
        content_type,
        size,
        content_hash,
        width,
        height,
        blurhash,
        is_animated,
        created_at
    )
SELECT
    attachment_id,
    content_type,
    size,
    content_hash,
    width,
    height,
    blurhash,
    is_animated,
    created_at
FROM
    attachment;

DROP TABLE attachment;

ALTER TABLE attachment_new
RENAME TO attachment;

DROP TABLE IF EXISTS pending_attachment;

CREATE INDEX IF NOT EXISTS idx_attachment_transfer_direction_state ON attachment_transfer (direction, state);

CREATE TRIGGER IF NOT EXISTS delete_unreferenced_attachment AFTER DELETE ON message_attachment FOR EACH ROW BEGIN
DELETE FROM attachment
WHERE
    attachment_id = OLD.attachment_id
    AND NOT EXISTS (
        SELECT
            1
        FROM
            message_attachment
        WHERE
            attachment_id = OLD.attachment_id
    );

END;

COMMIT;

PRAGMA foreign_keys = ON;
