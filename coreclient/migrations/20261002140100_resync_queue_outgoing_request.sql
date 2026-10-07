-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The unanswered connection request behind a connection group a new device
-- onboards into, serialized with the persistence codec. NULL unless the group
-- backs an outgoing request.
ALTER TABLE resync_queue ADD COLUMN outgoing_request BLOB;

-- The encrypted friendship package of the recipient's join, serialized with the
-- persistence codec. Set when the recipient accepts the outgoing request before
-- the new device onboards, since the device cannot process the join itself.
ALTER TABLE resync_queue ADD COLUMN accepted_friendship_package BLOB;
