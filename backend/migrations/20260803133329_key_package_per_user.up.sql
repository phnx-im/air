-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Key packages are stored now per user, not per client id.
--
ALTER TABLE key_package
ADD COLUMN user_id uuid;

UPDATE key_package kp
SET
    user_id = c.user_id
FROM
    qs_client_record c
WHERE
    c.client_id = kp.client_id;

-- safe: user_id IS NOT NULL because of the foreign key constraint
ALTER TABLE key_package ALTER COLUMN user_id
SET
    NOT NULL,
ADD FOREIGN KEY (user_id) REFERENCES qs_user_record (user_id) ON DELETE CASCADE,
DROP COLUMN client_id;

CREATE INDEX idx_key_package_user_id ON key_package (user_id);

ALTER TABLE apq_key_package
ADD COLUMN user_id uuid;

UPDATE apq_key_package kp
SET
    user_id = c.user_id
FROM
    qs_client_record c
WHERE
    c.client_id = kp.client_id;

-- safe: user_id IS NOT NULL because of the foreign key constraint
ALTER TABLE apq_key_package ALTER COLUMN user_id
SET
    NOT NULL,
ADD FOREIGN KEY (user_id) REFERENCES qs_user_record (user_id) ON DELETE CASCADE,
DROP COLUMN client_id;

CREATE INDEX idx_apq_key_package_user_id ON apq_key_package (user_id);
