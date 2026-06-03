-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

CREATE TABLE ds_collision_tag (
    group_id UUID    NOT NULL REFERENCES encrypted_group (group_id) ON DELETE CASCADE,
    epoch    BIGINT  NOT NULL,
    tag      BYTEA   NOT NULL,
    PRIMARY KEY (group_id, epoch, tag)
);
