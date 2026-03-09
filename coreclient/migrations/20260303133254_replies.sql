-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
--
-- Add content references for message and message draft replies
ALTER TABLE message ADD COLUMN in_reply_to_mimi_id BLOB;
ALTER TABLE message_draft ADD COLUMN in_reply_to_mimi_id BLOB;