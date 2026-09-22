-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Records the one token batch a user may draw per (operation type, allowance
-- epoch, key). Only the request hash is stored: replays resend the request and
-- the VOPRF evaluation is deterministic.
CREATE TABLE as_token_issuance (
    user_uuid uuid NOT NULL,
    user_domain TEXT NOT NULL,
    operation_type SMALLINT NOT NULL,
    allowance_epoch INTEGER NOT NULL,
    key_fingerprint BYTEA NOT NULL,
    request_hash BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (
        user_uuid, user_domain, operation_type, allowance_epoch,
        key_fingerprint
    ),
    FOREIGN KEY (user_uuid, user_domain)
        REFERENCES as_user_record (user_uuid, user_domain)
        ON DELETE CASCADE
);
