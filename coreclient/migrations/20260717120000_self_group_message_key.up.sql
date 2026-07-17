-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
CREATE TABLE self_group_message_key (
    group_id BLOB NOT NULL PRIMARY KEY,
    epoch BIGINT NOT NULL,
    key BLOB NOT NULL
);
