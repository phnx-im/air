-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
DROP TABLE IF EXISTS key_package;

DROP INDEX IF EXISTS idx_key_package_client_id;

DROP TABLE IF EXISTS qs_queue;

DROP TABLE IF EXISTS qs_queue_data;

DROP TABLE IF EXISTS qs_client_record;

DROP TABLE IF EXISTS qs_user_record;

DROP TABLE IF EXISTS qs_decryption_key;

DROP TABLE IF EXISTS qs_signing_key;

DROP TYPE IF EXISTS decryption_key_data;

DROP TYPE IF EXISTS signing_key_data;

