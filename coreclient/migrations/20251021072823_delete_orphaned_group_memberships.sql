-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Delete group memberships that do not have a corresponding group.

DELETE FROM group_membership
WHERE group_id NOT IN (SELECT group_id FROM "group");
