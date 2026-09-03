-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- One row per emulation binding and per derivation epoch log entry. Each row
-- keeps the per-epoch state of the derivation epoch it names alive.
--
-- This version also marks completion of the one-time
-- `RustMigration::VcDerivationEpochRetention` code migration, which converts
-- the old per-group records into rows. The next migration drops the old tables.

ALTER TABLE vc_emulation_binding RENAME TO vc_emulation_binding_record;

-- The derivation epoch is stored next to the opaque binding so that the sweep
-- can query by epoch.
CREATE TABLE vc_emulation_binding(
    group_id BLOB NOT NULL,
    group_epoch BLOB NOT NULL,
    epoch_id BLOB NOT NULL,
    binding BLOB NOT NULL,
    PRIMARY KEY (group_id, group_epoch)
);

CREATE INDEX vc_emulation_binding_epoch_id
    ON vc_emulation_binding (epoch_id);

CREATE TABLE vc_derivation_epoch_log_entry(
    group_id BLOB NOT NULL,
    epoch_id BLOB NOT NULL,
    entry BLOB NOT NULL,
    PRIMARY KEY (group_id, epoch_id)
);

CREATE INDEX vc_derivation_epoch_log_entry_epoch_id
    ON vc_derivation_epoch_log_entry (epoch_id);

-- Derivation epochs from before the log, which a queued sibling operation may
-- still reference. The sweep leaves a held epoch alone until `held_until` and
-- drops the hold afterwards.
CREATE TABLE vc_derivation_epoch_legacy_hold(
    epoch_id BLOB NOT NULL,
    held_until TEXT NOT NULL,
    PRIMARY KEY (epoch_id)
);
