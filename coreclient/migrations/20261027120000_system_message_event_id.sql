-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The ID that all devices of the user derive for a system message, which a
-- deletion on one device sends to the others. The paired code migration
-- backfills it. Content messages have none, which keeps the partial index
-- small.
ALTER TABLE message
ADD COLUMN event_id BLOB;

CREATE INDEX idx_message_event_id ON message (event_id)
WHERE
    event_id IS NOT NULL;
