-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Identifies this client (device), e.g. in self-group leaf credentials.
-- Generated at client creation. For existing clients the column is left NULL
-- here and backfilled by the application layer when the client DB is opened,
-- since sqlite cannot generate valid UUIDs.
ALTER TABLE own_client_info
ADD COLUMN client_id BLOB;
