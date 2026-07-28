-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Add a watermark up to which the chat's local notifications has been dismissed.
ALTER TABLE chat
ADD COLUMN notified_until DATETIME;

-- Backfill with `last_read`: everything read before the upgrade must not resurface as a
-- notification (in particular, historical reactions to own messages).
UPDATE chat
SET notified_until = last_read;
