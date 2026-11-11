-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The resync commit the DS has not confirmed yet, resent by the next attempt.
ALTER TABLE resync_queue ADD COLUMN pending_commit BLOB;
