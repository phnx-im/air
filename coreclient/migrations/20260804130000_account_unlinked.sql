-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Set when a sibling device removed this device from the self group. A single
-- row: a device can only be unlinked once, and the flag is terminal.
CREATE TABLE account_unlinked (
    id INTEGER NOT NULL PRIMARY KEY CHECK (id = 0)
);
