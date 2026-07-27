-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Identifies this client (device), e.g. in self-group leaf credentials.
-- Generated at client creation and backfilled with random bytes for existing
-- clients. Never NULL in practice, but ALTER TABLE cannot add a NOT NULL
-- column with a non-constant default.
ALTER TABLE own_client_info
ADD COLUMN client_id BLOB;

UPDATE own_client_info SET client_id = randomblob(16);
