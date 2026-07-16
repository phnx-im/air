-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Signing key used for this client's leaf in the self group. Distinct from the
-- client signing key so a linked device can present the shared client
-- credential while signing its self-group leaf with a fresh, unique key.
-- NULL for clients that sign their self-group leaf with their own client key.
ALTER TABLE own_client_info
ADD COLUMN self_group_signing_key BLOB;
