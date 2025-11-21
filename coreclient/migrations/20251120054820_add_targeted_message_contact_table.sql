-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

CREATE TABLE targeted_message_contact (
    -- We're not linking this to the user table, as we don't necessarily want
    -- this entry to be deleted in case we delete the user
    user_id BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    -- 1:1 relationship with chat
    chat_id BLOB NOT NULL UNIQUE,
    friendship_package_ear_key BLOB NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (user_id, user_domain),
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);