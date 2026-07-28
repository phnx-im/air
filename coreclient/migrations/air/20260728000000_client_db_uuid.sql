-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Random UUID naming a client's DB file, so that the file name does not leak
-- any metadata. NULL corresponds to the legacy DB file name derived from the
-- user id.
ALTER TABLE client_record ADD COLUMN db_uuid BLOB;
