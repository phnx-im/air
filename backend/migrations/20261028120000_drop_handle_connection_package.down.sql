-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE handle_connection_package (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash BYTEA NOT NULL,
    connection_package BYTEA NOT NULL,
    is_last_resort BOOLEAN,
    FOREIGN KEY (hash) REFERENCES as_user_handle (hash) ON DELETE CASCADE
);
