-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Rename the attachment id columns so the local client id is the primary
-- `attachment_id` and the server-assigned id is `remote_attachment_id`.
-- Rename the remote column first to free up the `attachment_id` name.
ALTER TABLE attachment RENAME COLUMN attachment_id TO remote_attachment_id;

ALTER TABLE attachment RENAME COLUMN local_attachment_id TO attachment_id;

-- The pending attachment is keyed by the server-assigned id.
ALTER TABLE pending_attachment RENAME COLUMN attachment_id TO remote_attachment_id;
