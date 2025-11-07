-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Add a new table to store blocked contacts.
--
CREATE TABLE blocked_contact (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    last_display_name TEXT NOT NULL,
    blocked_at TEXT NOT NULL,
    PRIMARY KEY (user_uuid, user_domain)
    -- Note: No foreign key constraint on the user/contact table, because we
    -- want to keep the blocked contact id when the user/contact is deleted.
);

CREATE INDEX idx_blocked_contact_blocked_at ON blocked_contact (blocked_at);
