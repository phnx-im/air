-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The contact behind a connection group a freshly linked device onboards into,
-- serialized with the persistence codec. NULL means the resync onboards into a
-- group chat.
ALTER TABLE resync_queue ADD COLUMN connection_contact BLOB;
