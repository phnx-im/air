-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
CREATE TABLE device_link (
    link_id BLOB NOT NULL PRIMARY KEY,
    state TEXT NOT NULL,
    -- The CBOR-encoded queue the existing device created for the new device.
    queue BLOB NOT NULL,
    -- The new device's client id, once it asked to join the self group.
    client_id BLOB,
    -- The CBOR-encoded join request, once the new device sent it.
    join_request BLOB,
    -- The CBOR-encoded reason the add failed for good, if it did.
    failure BLOB,
    -- When an unfinished link counts as abandoned.
    expires_at TEXT NOT NULL
);
