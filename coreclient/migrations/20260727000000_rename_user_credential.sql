-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

ALTER TABLE client_credential RENAME TO user_credential;
ALTER TABLE user_credential RENAME COLUMN client_credential TO user_credential;
DROP INDEX idx_client_credential_user_id;
CREATE INDEX idx_user_credential_user_id ON user_credential (user_uuid, user_domain);
