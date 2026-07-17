-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Add a watermark up to which the chat's local notifications has been dismissed.
ALTER TABLE chat
ADD COLUMN notified_until DATETIME;
