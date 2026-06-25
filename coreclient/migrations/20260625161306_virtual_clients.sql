-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
--
CREATE TABLE vc_emulation_group_secret(
    epoch_id BLOB NOT NULL,
    secret_type TEXT NOT NULL CHECK (secret_type IN (
        'pprf',
        'emulation_epoch_state'
    )),
    vc_secret BLOB NOT NULL,
    PRIMARY KEY (epoch_id, secret_type)
);


CREATE TABLE vc_emulation_binding(
    group_id BLOB NOT NULL,
    bindings BLOB NOT NULL,
    PRIMARY KEY (group_id)
);

CREATE TABLE vc_operation_tree(
    epoch_id BLOB NOT NULL,
    operation_tree BLOB NOT NULL,
    PRIMARY KEY (epoch_id)
);

CREATE TABLE vc_retained_key_package_material(
    key_package_ref BLOB NOT NULL,
    epoch_id BLOB NOT NULL,
    record BLOB NOT NULL,
    PRIMARY KEY (key_package_ref)
);

CREATE INDEX vc_retained_key_package_material_epoch_id
    ON vc_retained_key_package_material (epoch_id);
