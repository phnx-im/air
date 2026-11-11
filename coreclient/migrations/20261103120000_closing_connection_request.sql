-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Incoming username requests from a sender who is already a contact.
CREATE TABLE closing_connection_request(
    request_id BLOB NOT NULL PRIMARY KEY,
    sender_user_uuid BLOB NOT NULL,
    sender_user_domain text NOT NULL,
    connection_info BLOB NOT NULL,
    connection_offer_hash BLOB NOT NULL,
    created_at text NOT NULL,
    attempts integer NOT NULL DEFAULT 0,
    retry_at text
);

