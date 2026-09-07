-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- One row per registration attempt the adaptive registration gate evaluated,
-- which is what its per-address counter counts. Rows are pruned once they age
-- out of that counter's window, so a rolling window comes from row age alone.
--
-- The address is not stored. `ip_bucket` is a keyed hash of the address bucket
-- (/32 for IPv4, /64 for IPv6), which is all the per-address counter needs.
--
-- Unlogged: these rows are written on every registration attempt and are worth
-- nothing once they age out. Crash recovery truncates the table, which opens
-- the gate until the counters fill again.
CREATE UNLOGGED TABLE registration_attempt (
    ip_bucket BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_registration_attempt_ip_bucket_created_at
    ON registration_attempt (ip_bucket, created_at);

-- One row per completed registration that carried no challenge, which is what
-- the gate's deployment-wide counter counts. Pruned like the attempts above.
--
-- No address bucket, because a deployment-wide count needs none. A completed
-- registration therefore stores nothing derived from the registrant address.
--
-- Unlogged for the same reason as the attempts above.
CREATE UNLOGGED TABLE registration_record (
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_registration_record_created_at ON registration_record (created_at);

-- The key the buckets above are hashed under, generated on first start. A
-- single row, enforced by a primary key that only accepts TRUE. It lives in the
-- database rather than in memory so that every replica buckets alike.
--
-- Unlogged, like the rows it keys. Crash recovery truncates both together, so a
-- regenerated key never has records hashed under the old one to mismatch.
CREATE UNLOGGED TABLE registration_ip_bucket_key (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE,
    key BYTEA NOT NULL,
    CONSTRAINT registration_ip_bucket_key_singleton CHECK (singleton)
);
