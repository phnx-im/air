-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Depth-first index of the attachment among the attachment parts of the message
ALTER TABLE attachment
ADD COLUMN part_index INTEGER NOT NULL DEFAULT 0;

UPDATE attachment
SET
    part_index = (
        SELECT
            COUNT(*)
        FROM
            attachment AS earlier
        WHERE
            earlier.message_id = attachment.message_id
            AND earlier.rowid < attachment.rowid
    );

CREATE UNIQUE INDEX idx_attachment_message_part_index ON attachment (message_id, part_index);
