-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Parks every username change in the outbox for the next self-group commit.
INSERT INTO self_group_outbox(kind, key, payload)
SELECT
    'username',
    CAST(handle AS BLOB),
    x''
FROM
    user_handle
WHERE
    EXISTS (
        SELECT
            1
        FROM
            own_client_info
        WHERE
            self_group_id IS NOT NULL);

