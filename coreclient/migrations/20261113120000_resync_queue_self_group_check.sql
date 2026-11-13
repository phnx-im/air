-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Turn every queued resync of the own self group that the user did not request
-- into a pending self-group check.
UPDATE resync_queue
SET
    reason = 'self_group_check',
    status = 'pending',
    attempts = 0,
    not_before = NULL,
    last_error = NULL
WHERE reason != 'manual'
    AND group_id IN (
        SELECT self_group_id FROM own_client_info WHERE self_group_id IS NOT NULL
    );
