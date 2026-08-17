-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The emulation epoch an emulation group registered for its current group
-- epoch. One row per emulation group, holding the serialized registration
-- record (group epoch plus derived emulation epoch id). Written by
-- register_vc_emulation_epoch so a repeated call in the same group epoch
-- returns the existing epoch id instead of consuming the forward-secure
-- exporter again.
CREATE TABLE vc_registered_emulation_epoch(
    group_id BLOB NOT NULL,
    registration BLOB NOT NULL,
    PRIMARY KEY (group_id)
);
