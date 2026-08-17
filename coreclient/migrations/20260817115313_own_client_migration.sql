-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

CREATE TABLE own_client_migration (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    migration INTEGER NOT NULL,
    PRIMARY KEY (user_uuid, user_domain, migration),
    FOREIGN KEY (user_uuid, user_domain) REFERENCES own_client_info (user_uuid, user_domain) ON DELETE CASCADE
);
