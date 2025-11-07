-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Encrypted group states
CREATE TABLE encrypted_group(
    group_id uuid PRIMARY KEY,
    ciphertext BYTEA NOT NULL,
    last_used timestamptz NOT NULL,
    deleted_queues BYTEA NOT NULL
);