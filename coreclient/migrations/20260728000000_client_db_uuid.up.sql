-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Mirrors the air DB migration of the same name so that sqlx can type-check
-- queries against the `client_record` table.
ALTER TABLE client_record ADD COLUMN db_uuid BLOB;
