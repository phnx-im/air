-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The user's not-yet-synchronized setting changes. A single row: there is at
-- most one self-group to sync through.
CREATE TABLE setting_changes (
    id INTEGER NOT NULL PRIMARY KEY CHECK (id = 0),
    changes BLOB NOT NULL,
    previous BLOB NOT NULL
);
