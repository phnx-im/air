-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Recreate indices and triggers for group_membership table that were dropped in
-- the 'resync_queue' migration.
CREATE INDEX idx_group_membership_user_id ON group_membership (user_uuid, user_domain);

CREATE TRIGGER delete_orphaned_data AFTER DELETE ON group_membership FOR EACH ROW BEGIN
-- Delete user profiles of users that are not in any group and that are not our own.
DELETE FROM user
WHERE
    user_uuid = OLD.user_uuid
    AND user_domain = OLD.user_domain
    AND NOT EXISTS (
        SELECT
            1
        FROM
            group_membership
        WHERE
            user_uuid = OLD.user_uuid
            AND user_domain = OLD.user_domain
    )
    AND NOT EXISTS (
        SELECT
            1
        FROM
            own_client_info
        WHERE
            user_uuid = OLD.user_uuid
            AND user_domain = OLD.user_domain
    );
END;