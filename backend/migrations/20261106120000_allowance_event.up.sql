-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- One row per charge against a rolling-window allowance. A row counts
-- against its key until `expires_at`.
--
CREATE UNLOGGED TABLE allowance_event (
    key_value BYTEA NOT NULL,
    cost BIGINT NOT NULL CHECK (cost > 0),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX allowance_event_key ON allowance_event (key_value, expires_at);
