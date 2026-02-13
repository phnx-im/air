-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
--
-- Scheduled operation executed as a job
--
-- Operations can be retried on non-fatal errors.
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
