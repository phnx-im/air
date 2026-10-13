-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Incoming connection requests the user declined, by request id. Copies of a
-- declined request that arrives later are dropped.
CREATE TABLE declined_connection_request(
    request_id BLOB NOT NULL PRIMARY KEY,
    declined_at text NOT NULL
);

