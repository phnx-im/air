-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
DROP TABLE IF EXISTS as_signing_key;

DROP INDEX IF EXISTS idx_connection_package_user_uuid;

DROP TABLE IF EXISTS as_client_record;

DROP TABLE IF EXISTS as_user_record;

DROP TABLE IF EXISTS as_batched_key;

DROP TYPE IF EXISTS credential_type;

DROP TYPE IF EXISTS client_credential;

DROP TYPE IF EXISTS expiration;

DROP TYPE IF EXISTS indexed_ciphertext;

DROP TYPE IF EXISTS aead_ciphertext;

DROP TABLE IF EXISTS handle_connection_package;

DROP TABLE IF EXISTS allowance_record;

DROP TABLE IF EXISTS as_user_handles_queue;

DROP INDEX IF EXISTS as_user_handles_queues_created_at;

DROP INDEX IF EXISTS as_user_handles_queues_fetched_by;

DROP TABLE IF EXISTS as_user_handle;
