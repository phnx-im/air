-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Groups whose pending operation failed before pending_commit_failed existed
-- are parked in waiting_for_queue_response and never contact the DS again, so
-- the flag would never be set for them without this backfill.
UPDATE "group"
SET pending_commit_failed = 1
WHERE group_id IN (
    SELECT group_id
    FROM pending_chat_operation
    WHERE request_status = 'waiting_for_queue_response'
);
