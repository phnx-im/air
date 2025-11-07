-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

CREATE TYPE signing_key_data AS (
    signing_key BYTEA,
    verifying_key BYTEA
);

CREATE TABLE qs_signing_key (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    signing_key signing_key_data NOT NULL
);

CREATE TYPE decryption_key_data AS (
    encryption_key BYTEA,
    decryption_key BYTEA
);

CREATE TABLE qs_decryption_key (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    decryption_key decryption_key_data NOT NULL
);

CREATE TABLE qs_user_record (
    user_id uuid PRIMARY KEY,
    friendship_token BYTEA UNIQUE NOT NULL,
    verifying_key BYTEA NOT NULL
);

CREATE TABLE qs_client_record (
    client_id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    encrypted_push_token aead_ciphertext,
    owner_public_key BYTEA NOT NULL,
    owner_signature_key BYTEA NOT NULL,
    ratchet BYTEA NOT NULL,
    activity_time timestamptz NOT NULL,
    FOREIGN KEY (user_id) REFERENCES qs_user_record(user_id) ON DELETE CASCADE
);

CREATE TABLE qs_queue_data (
    queue_id uuid PRIMARY KEY,
    sequence_number BIGINT NOT NULL,
    FOREIGN KEY (queue_id) REFERENCES qs_client_record(client_id) ON DELETE CASCADE
);

CREATE TABLE qs_queues (
    queue_id uuid NOT NULL,
    sequence_number BIGINT NOT NULL,
    message_bytes BYTEA NOT NULL,
    PRIMARY KEY (queue_id, sequence_number),
    FOREIGN KEY (queue_id) REFERENCES qs_queue_data(queue_id) ON DELETE CASCADE
);

CREATE TABLE key_package (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    client_id uuid NOT NULL,
    key_package BYTEA NOT NULL,
    is_last_resort BOOLEAN NOT NULL,
    FOREIGN KEY (client_id) REFERENCES qs_client_record(client_id) ON DELETE CASCADE
);

CREATE INDEX idx_key_package_client_id ON key_package(client_id);