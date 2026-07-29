-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Identifies this client (device), e.g. in self-group leaf credentials.
-- Generated at client creation. For existing clients the column defaults to
-- the nil UUID and is backfilled by the application layer when the client DB
-- is opened, since sqlite cannot generate valid UUIDs.
ALTER TABLE own_client_info
ADD COLUMN client_id BLOB NOT NULL DEFAULT x'00000000000000000000000000000000';
