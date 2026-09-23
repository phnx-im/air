-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The outbox of self-group messages parked to reach the user's other devices.
CREATE TABLE self_group_outbox (
    kind TEXT NOT NULL,
    key BLOB NOT NULL,
    payload BLOB NOT NULL,
    previous BLOB,
    PRIMARY KEY (kind, key)
    -- Note: No foreign key constraint: a parked change must outlive the deletion
    -- of the resource it was related to.
);

DROP TABLE setting_changes;
DROP TABLE blocked_contact_change;
