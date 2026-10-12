-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The group id of the group chat a targeted connection request went through,
-- so a new device can show where the request came from. NULL for requests sent
-- before it was kept. No foreign key, since the group chat may be deleted.
ALTER TABLE targeted_message_contact ADD COLUMN origin_group_id BLOB;
