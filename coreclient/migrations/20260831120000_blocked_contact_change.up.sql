-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The outbox of blocked-contact changes that still have to reach the other devices.
CREATE TABLE blocked_contact_change (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    -- Both columns NULL means unblocked.
    blocked_at INTEGER,
    last_display_name TEXT,
    PRIMARY KEY (user_uuid, user_domain),
    CHECK ((blocked_at IS NULL) = (last_display_name IS NULL))
    -- Note: No foreign key constraint on the user/contact table, for the same
    -- reason as in `blocked_contact`: the parked change must outlive the
    -- deletion of the user/contact.
);
