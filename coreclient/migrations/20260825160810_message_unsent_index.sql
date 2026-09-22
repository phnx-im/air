-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--

-- Speeds up looking up attachments by status, optionally restricted or ordered
-- by creation time (stale uploads, pending downloads).
CREATE INDEX IF NOT EXISTS idx_attachment_status_created_at ON attachment (status, created_at);
DROP INDEX IF EXISTS idx_attachment_pending_ordered;

-- Speeds up finding messages that have not been sent yet. Unsent messages are
-- transient and few, so the partial index stays tiny.
CREATE INDEX IF NOT EXISTS idx_message_unsent ON message (message_id, chat_id, status)
WHERE
    sent = 0;
