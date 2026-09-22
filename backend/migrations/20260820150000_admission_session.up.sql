-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
-- A push-admission session, deleted once a registration carries its challenge
-- back, so a challenge admits one account.
--
-- The push token is never stored, only a keyed hash of it.
CREATE UNLOGGED TABLE admission_session (
    session_id uuid PRIMARY KEY,
    challenge_hash bytea NOT NULL,
    endpoint_bucket bytea NOT NULL,
    platform text NOT NULL,
    expires_at timestamptz NOT NULL
);

CREATE INDEX idx_admission_session_expires_at ON admission_session (expires_at);

-- One row per challenge sent to an endpoint, which bounds the push traffic an
-- unauthenticated request can aim at somebody else's endpoint.
CREATE UNLOGGED TABLE admission_send_record (
    endpoint_bucket bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now ()
);

CREATE INDEX idx_admission_send_record_created_at ON admission_send_record (created_at);

CREATE INDEX idx_admission_send_record_bucket_created_at ON admission_send_record (endpoint_bucket, created_at);

-- One row per registration an endpoint admitted, which is what the quotas
-- count.
CREATE UNLOGGED TABLE admission_consumption_record (
    endpoint_bucket bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now ()
);

CREATE INDEX idx_admission_consumption_record_created_at ON admission_consumption_record (created_at);

CREATE INDEX idx_admission_consumption_record_bucket_created_at ON admission_consumption_record (endpoint_bucket, created_at);

-- The key the endpoint buckets are hashed under, generated on first start.
CREATE UNLOGGED TABLE admission_endpoint_bucket_key (
    singleton boolean PRIMARY KEY DEFAULT TRUE,
    KEY BYTEA NOT NULL,
    CONSTRAINT admission_endpoint_bucket_key_singleton CHECK (singleton)
);
