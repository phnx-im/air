-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

CREATE TABLE user_credential_by_user_id (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    fingerprint BLOB NOT NULL,
    user_credential BLOB NOT NULL,
    PRIMARY KEY (user_uuid, user_domain)
);

-- Keeps the oldest credential of each user id.
INSERT INTO user_credential_by_user_id (user_uuid, user_domain, fingerprint, user_credential)
SELECT
    user_uuid,
    user_domain,
    fingerprint,
    user_credential
FROM
    user_credential
WHERE
    rowid IN (
        SELECT
            MIN(rowid)
        FROM
            user_credential
        GROUP BY
            user_uuid,
            user_domain
    );

DROP TABLE user_credential;

ALTER TABLE user_credential_by_user_id RENAME TO user_credential;
