-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Records that a token batch was fetched for an allowance epoch.
--
-- Consumed tokens are deleted, so the absence of tokens says nothing about
-- whether the batch was ever fetched. Without this table a client that spent
-- its batch would keep re-requesting it.
CREATE TABLE privacy_pass_batch (
    operation_type INTEGER NOT NULL,
    key_fingerprint BLOB NOT NULL,
    allowance_epoch INTEGER NOT NULL,
    fetched_at DATETIME NOT NULL,
    PRIMARY KEY (operation_type, key_fingerprint, allowance_epoch)
);
