-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Add 'application_export_tree' constraint to group_data.data_type
ALTER TABLE group_data
RENAME TO group_data_old;

CREATE TABLE group_data (
    group_id BLOB NOT NULL,
    data_type TEXT NOT NULL CHECK (
        data_type IN (
            'join_group_config',
            'tree',
            'interim_transcript_hash',
            'context',
            'confirmation_tag',
            'group_state',
            'message_secrets',
            'resumption_psk_store',
            'own_leaf_index',
            'use_ratchet_tree_extension',
            'group_epoch_secrets',
            'application_export_tree'
        )
    ),
    group_data BLOB NOT NULL,
    PRIMARY KEY (group_id, data_type)
);

INSERT INTO
    group_data
SELECT
    *
FROM
    group_data_old;

DROP TABLE group_data_old;
