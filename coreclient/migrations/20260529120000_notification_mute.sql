-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Add muted_until column to the chat table for per-chat notification muting.
--
-- NULL means not muted. A datetime value means muted until that point in time.
-- The sentinel '9999-01-01T00:00:00Z' is used for "muted indefinitely".
--
ALTER TABLE chat ADD COLUMN muted_until DATETIME;
