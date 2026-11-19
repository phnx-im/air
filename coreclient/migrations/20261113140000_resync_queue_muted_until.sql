-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The mute of the chat a new device onboards into, applied once the chat
-- exists. Initially set during linking. NULL while the chat is not muted.
ALTER TABLE resync_queue
    ADD COLUMN muted_until DATETIME;

