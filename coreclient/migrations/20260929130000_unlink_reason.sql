-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Records why this device was reset, not only that it was.
--
ALTER TABLE own_client_info
ADD COLUMN unlink_reason TEXT;

UPDATE own_client_info
SET
    unlink_reason = 'unlinked'
WHERE
    unlinked = TRUE;

ALTER TABLE own_client_info
DROP COLUMN unlinked;
