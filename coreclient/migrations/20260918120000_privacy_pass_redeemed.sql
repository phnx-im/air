-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- A table to track the redeemed privacy pass tokens. We only store the index
-- (position inside a batch), because the issuance is deterministic and
-- idempotent.
CREATE TABLE privacy_pass_redeemed(
    operation_type integer NOT NULL,
    key_fingerprint BLOB NOT NULL,
    allowance_epoch integer NOT NULL,
    token_index integer NOT NULL,
    -- To avoid easy correlation of redemption, we don't sync immediately.
    broadcast_after DATETIME,
    PRIMARY KEY (operation_type, key_fingerprint, allowance_epoch, token_index)
);

-- The key a batch token was issued under.
ALTER TABLE privacy_pass_token
    ADD COLUMN key_fingerprint BLOB;

