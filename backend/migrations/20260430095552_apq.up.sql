-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
CREATE TABLE apq_key_package (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    client_id uuid NOT NULL,
    key_package BYTEA NOT NULL,
    is_last_resort BOOLEAN NOT NULL,
    FOREIGN KEY (client_id) REFERENCES qs_client_record (client_id) ON DELETE CASCADE
);

CREATE INDEX idx_apq_key_package_client_id ON apq_key_package (client_id);
