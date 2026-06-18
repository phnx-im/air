-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- A batch of staged key packages
--
-- The actual staged key packages are stored in the `qs_staged_key_package` table.
CREATE TABLE qs_staged_key_package_batch (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id uuid NOT NULL,
    epoch_id BYTEA NOT NULL,
    random BYTEA NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now (),
    UNIQUE (user_id, epoch_id, random),
    FOREIGN KEY (user_id) REFERENCES qs_user_record (user_id) ON DELETE CASCADE
);

CREATE INDEX qs_staged_key_package_batch_created_at ON qs_staged_key_package_batch (created_at);

-- Staged key package for a given batch
CREATE TABLE qs_staged_key_package (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    batch_id INTEGER NOT NULL,
    key_package BYTEA NOT NULL, -- TLS blob; APQ: t_kp.tls || pq_kp.tls
    is_last_resort BOOLEAN NOT NULL,
    is_apq BOOLEAN NOT NULL,
    FOREIGN KEY (batch_id) REFERENCES qs_staged_key_package_batch (id) ON DELETE CASCADE
);

CREATE INDEX qs_staged_key_package_batch_id ON qs_staged_key_package (batch_id);
