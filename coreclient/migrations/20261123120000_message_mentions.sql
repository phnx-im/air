-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Whether the message body mentions the local user
ALTER TABLE message
    ADD COLUMN mentions_me boolean NOT NULL DEFAULT FALSE;

-- Partial index to count mentions
CREATE INDEX idx_message_mentions_me ON message(chat_id, timestamp)
WHERE
    mentions_me;

-- Mentions of chat members
CREATE TABLE message_mention(
    message_id BLOB NOT NULL,
    position integer NOT NULL,
    user_uuid BLOB NOT NULL,
    user_domain text NOT NULL,
    PRIMARY KEY (message_id, position),
    FOREIGN KEY (message_id) REFERENCES message(message_id) ON DELETE CASCADE
);

