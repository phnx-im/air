-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Tasks scheduled for specific times.
CREATE TABLE timed_tasks_queue (
    -- Marking additionally as NOT NULL because otherwise sqlx gets confused.
    task_kind TEXT PRIMARY KEY NOT NULL,
    locked_by BLOB,
    due_at TEXT NOT NULL
);

CREATE INDEX idx_timed_tasks_queue_due_at ON timed_tasks_queue (due_at);

-- KeyPackageRefs of KeyPackages that have been uploaded in the past. This is to
-- track which KeyPackages which were uploaded in the most recent two rounds.
-- Older KeyPackages can be deleted.
CREATE TABLE key_package_refs (
    key_package_ref BLOB PRIMARY KEY,
    -- true if KeyPackage is on the server, false if it was deleted from the
    -- server, but may still be used in edge cases.
    is_live INTEGER NOT NULL,
    -- This is to ensure that when OpenMLS deletes a KeyPackage because it was
    -- used, the entry in this table is cleaned up as well.
    FOREIGN KEY (key_package_ref) REFERENCES key_package (key_package_ref) ON DELETE CASCADE
);

CREATE INDEX idx_key_package_ref_is_live ON key_package_refs (is_live);
