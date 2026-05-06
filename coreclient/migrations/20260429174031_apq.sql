-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
CREATE TABLE apq_key_package_refs (
    key_package_ref BLOB PRIMARY KEY,
    is_live INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_apq_key_package_ref_is_live ON apq_key_package_refs (is_live);

CREATE TABLE pq_group (
    group_id BLOB PRIMARY KEY NOT NULL,
    t_group_id BLOB NOT NULL,
    group_state_ear_key BLOB NOT NULL,
    self_updated_at INTEGER NOT NULL,
    FOREIGN KEY (t_group_id) REFERENCES "group" (group_id) ON DELETE CASCADE
);

CREATE INDEX idx_pq_group_t_group_id ON pq_group (t_group_id);
