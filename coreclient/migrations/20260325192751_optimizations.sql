-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Missing or redundant indices
CREATE INDEX IF NOT EXISTS idx_receipt_queue_locked_by ON receipt_queue (locked_by)
WHERE
    locked_by IS NOT NULL;

DROP INDEX IF EXISTS idx_message_chat_id;

DROP INDEX IF EXISTS idx_message_timetstamp;

CREATE INDEX IF NOT EXISTS idx_message_non_system ON message (chat_id, timestamp DESC)
WHERE
    sender_user_uuid IS NOT NULL
    AND sender_user_domain IS NOT NULL;

-- Automatically count unread messages
ALTER TABLE chat
ADD COLUMN unread_count INTEGER DEFAULT 0;

-- Backfill unread_count for existing chats
UPDATE chat
SET
    unread_count = (
        SELECT COUNT(*)
        FROM message
        WHERE
            message.chat_id = chat.chat_id
            AND message.sender_user_uuid IS NOT NULL
            AND message.sender_user_domain IS NOT NULL
            AND message.status != 1
            AND message.timestamp > chat.last_read
    );

CREATE TRIGGER IF NOT EXISTS chat_increment_unread_count AFTER INSERT ON message FOR EACH ROW WHEN (
    NEW.sender_user_uuid IS NOT NULL
    AND NEW.sender_user_domain IS NOT NULL
    -- exclude deleted messages
    AND NEW.status != 1
) BEGIN
UPDATE chat
SET
    unread_count = unread_count + 1
WHERE
    chat_id = NEW.chat_id
    AND last_read < NEW.timestamp;

END;

CREATE TRIGGER IF NOT EXISTS chat_decrement_unread_on_delete AFTER
UPDATE OF status ON message
WHEN NEW.status = 1
    AND OLD.status != 1
    AND NEW.sender_user_uuid IS NOT NULL
    AND NEW.sender_user_domain IS NOT NULL
BEGIN
UPDATE chat
SET
    unread_count = MAX(0, unread_count - 1)
WHERE
    chat_id = NEW.chat_id
    AND last_read < NEW.timestamp;

END;

CREATE TRIGGER IF NOT EXISTS chat_reset_unread_count AFTER
UPDATE OF last_read ON chat BEGIN
UPDATE chat
SET
    unread_count = (
        SELECT COUNT(*)
        FROM message
        WHERE
            message.chat_id = NEW.chat_id
            AND message.sender_user_uuid IS NOT NULL
            AND message.sender_user_domain IS NOT NULL
            AND message.status != 1
            AND message.timestamp > NEW.last_read
    )
WHERE
    chat_id = NEW.chat_id;

END;

-- Optimize operation table by removing nullable fields
CREATE TABLE operation_new (
    operation_id BLOB NOT NULL PRIMARY KEY,
    kind TEXT NOT NULL,
    data BLOB NOT NULL,
    created_at TEXT NOT NULL,
    scheduled_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z',
    retries INTEGER NOT NULL DEFAULT 0,
    locked_by BLOB NOT NULL DEFAULT x'00000000000000000000000000000000'
);

INSERT INTO
    operation_new (
        operation_id,
        kind,
        data,
        created_at,
        scheduled_at,
        retries,
        locked_by
    )
SELECT
    operation_id,
    kind,
    data,
    created_at,
    COALESCE(scheduled_at, '1970-01-01T00:00:00Z'),
    retries,
    COALESCE(locked_by, x'00000000000000000000000000000000')
FROM
    operation;

DROP TABLE operation;

ALTER TABLE operation_new
RENAME TO operation;

CREATE INDEX idx_operation_worker_queue ON operation (kind, scheduled_at, created_at, locked_by);
