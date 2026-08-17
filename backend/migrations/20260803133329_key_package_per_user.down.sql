-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
ALTER TABLE key_package
ADD COLUMN client_id uuid;

-- Attach the packages to an arbitrary live client of the user
UPDATE key_package kp
SET
    client_id = c.client_id
FROM
    (
        SELECT DISTINCT
            ON (user_id) user_id,
            client_id
        FROM
            qs_client_record
        ORDER BY
            user_id,
            (deleted_at IS NOT NULL),
            client_id
    ) c
WHERE
    c.user_id = kp.user_id;

-- Users without any client cannot be represented in the old schema
DELETE FROM key_package
WHERE
    client_id IS NULL;

ALTER TABLE key_package ALTER COLUMN client_id
SET
    NOT NULL,
ADD FOREIGN KEY (client_id) REFERENCES qs_client_record (client_id) ON DELETE CASCADE,
DROP COLUMN user_id;

CREATE INDEX idx_key_package_client_id ON key_package (client_id);

ALTER TABLE apq_key_package
ADD COLUMN client_id uuid;


-- Attach the packages to an arbitrary live client of the user
UPDATE apq_key_package kp
SET
    client_id = c.client_id
FROM
    (
        SELECT DISTINCT
            ON (user_id) user_id,
            client_id
        FROM
            qs_client_record
        ORDER BY
            user_id,
            (deleted_at IS NOT NULL),
            client_id
    ) c
WHERE
    c.user_id = kp.user_id;

-- Users without any client cannot be represented in the old schema
DELETE FROM apq_key_package
WHERE
    client_id IS NULL;

ALTER TABLE apq_key_package ALTER COLUMN client_id
SET
    NOT NULL,
ADD FOREIGN KEY (client_id) REFERENCES qs_client_record (client_id) ON DELETE CASCADE,
DROP COLUMN user_id;

CREATE INDEX idx_apq_key_package_client_id ON apq_key_package (client_id);
