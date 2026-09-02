-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Connection accept scheduled for being performed by the outbound service.
-- The row persists until the join lands or the chat is deleted. A permanent
-- failure is recorded in failed_reason and pauses the retries.
CREATE TABLE IF NOT EXISTS connection_accept_queue (
    chat_id BLOB NOT NULL,
    locked_by BLOB,
    failed_reason TEXT,
    PRIMARY KEY (chat_id),
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);
