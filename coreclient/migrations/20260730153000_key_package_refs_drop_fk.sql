-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Drop the foreign key from key_package_refs to key_package.
--
-- The table tracks which key packages the server serves for us, which is not
-- the same as which key package bundles we hold locally: a key package batch
-- uploaded by a sibling emulator of the same virtual client is announced in
-- the SafeAAD of a self-group commit and reproduced locally as retained
-- virtual-client material (a per-package seed in the OpenMLS store), never as
-- a key_package row. The foreign key made recording such a batch fail.
--
-- The FK also cleaned the table up: deleting a bundle cascaded its ref row
-- away. mark_key_packages_as_live now deletes stale ref rows explicitly,
-- which also fixes the same unbounded growth in apq_key_package_refs, which
-- never had a foreign key.
CREATE TABLE key_package_refs_new (
    key_package_ref BLOB PRIMARY KEY,
    -- true if the key package is on the server, false if it was replaced
    -- there, but may still be needed to decrypt an in-flight welcome.
    is_live INTEGER NOT NULL
);

INSERT INTO key_package_refs_new (key_package_ref, is_live)
SELECT key_package_ref, is_live FROM key_package_refs;

DROP TABLE key_package_refs;

ALTER TABLE key_package_refs_new RENAME TO key_package_refs;

CREATE INDEX idx_key_package_ref_is_live ON key_package_refs (is_live);
