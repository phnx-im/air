-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE username_signed_connection_package (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash BYTEA NOT NULL,
    -- Proto-encoded SignedConnectionPackagePayload, stored exactly as
    -- received. The signature and the package hash are computed over these
    -- bytes, so they must never be re-encoded.
    payload BYTEA NOT NULL,
    signature BYTEA NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    is_last_resort BOOLEAN NOT NULL DEFAULT FALSE,
    FOREIGN KEY (hash) REFERENCES as_user_handle (hash) ON DELETE CASCADE,
    UNIQUE (hash, payload)
);
