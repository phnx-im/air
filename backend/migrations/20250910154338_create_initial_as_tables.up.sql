-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE as_batched_key (
    token_key_id smallint PRIMARY KEY,
    voprf_server BYTEA NOT NULL
);

CREATE TYPE aead_ciphertext AS (ciphertext BYTEA, nonce BYTEA);

CREATE TYPE indexed_ciphertext AS (ciphertext aead_ciphertext, key_index BYTEA);

CREATE TABLE as_user_record (
    user_uuid uuid NOT NULL,
    user_domain TEXT NOT NULL,
    encrypted_user_profile indexed_ciphertext NOT NULL,
    staged_user_profile indexed_ciphertext,
    PRIMARY KEY (user_uuid, user_domain)
);

CREATE TYPE expiration AS (not_before timestamptz, not_after timestamptz);

CREATE TYPE client_credential AS (
    version BYTEA,
    signature_scheme BYTEA,
    verifying_key BYTEA,
    expiration_data expiration,
    signer_fingerprint BYTEA,
    signature BYTEA
);

CREATE TABLE as_client_record (
    user_uuid uuid PRIMARY KEY,
    user_domain TEXT NOT NULL,
    activity_time timestamptz NOT NULL,
    credential client_credential NOT NULL,
    remaining_tokens integer NOT NULL,
    FOREIGN KEY (user_uuid, user_domain) REFERENCES as_user_record (user_uuid, user_domain) ON DELETE CASCADE
);

CREATE TYPE credential_type AS ENUM ('as', 'intermediate');

CREATE TABLE as_signing_key (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    cred_type credential_type NOT NULL,
    credential_fingerprint BYTEA NOT NULL,
    signing_key BYTEA NOT NULL,
    currently_active boolean NOT NULL
);

CREATE TABLE IF NOT EXISTS as_user_handle (
    hash BYTEA PRIMARY KEY,
    verifying_key BYTEA NOT NULL,
    expiration_data expiration NOT NULL
);

CREATE TABLE IF NOT EXISTS as_user_handles_queue (
    message_id uuid PRIMARY KEY,
    hash BYTEA NOT NULL,
    message_bytes BYTEA NOT NULL,
    fetched_by uuid,
    created_at timestamptz DEFAULT now (),
    FOREIGN KEY (hash) REFERENCES as_user_handle (hash) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS as_user_handles_queues_created_at ON as_user_handles_queue (created_at);

CREATE INDEX IF NOT EXISTS as_user_handles_queues_fetched_by ON as_user_handles_queue (hash, fetched_by)
WHERE
    fetched_by IS NOT NULL;

CREATE UNLOGGED TABLE allowance_record(
    key_value bytea PRIMARY KEY,
    remaining bigint NOT NULL,
    valid_until timestamptz NOT NULL
);

CREATE TABLE handle_connection_package (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    hash BYTEA NOT NULL,
    connection_package BYTEA NOT NULL,
    FOREIGN KEY (hash) REFERENCES as_user_handle (hash) ON DELETE CASCADE
);
