-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
CREATE TABLE client_record_old (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    record_state TEXT NOT NULL CHECK (record_state IN ('in_progress', 'finished')),
    created_at DATETIME NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (user_uuid, user_domain)
);

INSERT OR IGNORE INTO client_record_old (user_uuid, user_domain, record_state, created_at, is_default)
SELECT user_uuid, user_domain, record_state, created_at, is_default
FROM client_record
ORDER BY created_at DESC;

DROP TABLE client_record;

ALTER TABLE client_record_old RENAME TO client_record;
