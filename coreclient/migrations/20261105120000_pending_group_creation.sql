-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

CREATE TABLE pending_group_creation (
    group_id BLOB NOT NULL PRIMARY KEY,
    title TEXT NOT NULL,
    picture BLOB,
    created_at TEXT NOT NULL,
    FOREIGN KEY (group_id) REFERENCES "group" (group_id) ON DELETE CASCADE
);
