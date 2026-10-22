-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
DELETE FROM qs_queues
WHERE queue_id IN (
    SELECT c.client_id
    FROM qs_client_record c
    JOIN qs_user_record u ON u.user_id = c.user_id
    WHERE u.deleted_at IS NOT NULL
);

DELETE FROM qs_user_record
WHERE deleted_at IS NOT NULL;

ALTER TABLE qs_user_record
DROP COLUMN deleted_at;
