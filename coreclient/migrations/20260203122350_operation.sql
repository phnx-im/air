-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
--
-- Scheduled operation to be executed
--
-- `operation_id` is opaque and is not necessarily a UUID. It can be
-- deterministiccally generated from data or be random.
--
-- Operations with `scheduled_at` NULL should be executed as soon as possible.
--
-- Operations with the same `kind` form a queue ordered by scheduled_at and
-- then by created_at. `kind` also specifies the format of the `data` field.
CREATE TABLE operation (
    operation_id BLOB NOT NULL PRIMARY KEY,
    kind TEXT NOT NULL,
    data BLOB NOT NULL,
    created_at TEXT NOT NULL,
    scheduled_at TEXT,
    retries INTEGER NOT NULL DEFAULT 0,
    locked_by BLOB
);

CREATE INDEX idx_operation_claim ON operation (kind, scheduled_at, created_at)
WHERE
    locked_by IS NULL;

-- This table is replaced by the more general operation table
DROP TABLE IF EXISTS timed_tasks_queue;
