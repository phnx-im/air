-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

CREATE TABLE ds_welcome_info (
    group_id   UUID        NOT NULL REFERENCES encrypted_group (group_id) ON DELETE CASCADE,
    epoch      BIGINT      NOT NULL,
    ciphertext BYTEA       NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (group_id, epoch)
);

CREATE INDEX ds_welcome_info_created_at ON ds_welcome_info (group_id, created_at);
