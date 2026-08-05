-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Rebuild client_record so that a user can have multiple client records, each
-- pointing to its own client DB file.
--
-- client_record_id is a random UUID naming the client's DB file, so that the file name
-- does not leak any metadata. Legacy records get a random UUID assigned here;
-- their DB files (named after the user id) are renamed when the air DB is
-- opened.
CREATE TABLE client_record_new (
    client_record_id BLOB NOT NULL PRIMARY KEY,
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    record_state TEXT NOT NULL CHECK (record_state IN ('in_progress', 'finished')),
    created_at DATETIME NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT FALSE
);

-- The generated blob is a valid UUIDv4: byte 6 starts with the version nibble
-- 4, byte 8 with one of the variant nibbles 8, 9, A or B.
INSERT INTO client_record_new (client_record_id, user_uuid, user_domain, record_state, created_at, is_default)
SELECT
    unhex(
        hex(randomblob(6))
        || '4' || substr(hex(randomblob(2)), 2)
        || substr('89AB', 1 + (abs(random()) % 4), 1) || substr(hex(randomblob(2)), 2)
        || hex(randomblob(6))
    ),
    user_uuid, user_domain, record_state, created_at, is_default
FROM client_record;

DROP TABLE client_record;

ALTER TABLE client_record_new RENAME TO client_record;
