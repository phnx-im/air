-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Count the server errors a queued item ran into and when it is due again, so
-- it is given up on after a few attempts.
ALTER TABLE chat_message_queue
ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;

ALTER TABLE chat_message_queue
ADD COLUMN retry_at TEXT;

ALTER TABLE reaction_queue
ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;

ALTER TABLE reaction_queue
ADD COLUMN retry_at TEXT;

ALTER TABLE receipt_queue
ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;

ALTER TABLE receipt_queue
ADD COLUMN retry_at TEXT;

-- Messages and reactions of a chat are sent in order.
CREATE INDEX idx_chat_message_queue_chat_id ON chat_message_queue (chat_id, created_at);

CREATE INDEX idx_reaction_queue_chat_id ON reaction_queue (chat_id, created_at);
