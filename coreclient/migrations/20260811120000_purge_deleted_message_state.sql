-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Clean up after messages deleted before a deletion cleaned up after itself.
--
-- A reaction is only ever stored for a message we have, so a target matching no
-- message is one whose Mimi ID an edit or a deletion rewrote. This keys on the
-- dangling reference rather than on the deleted status, which a peer's status
-- report may have overwritten. The remaining per-row state a deletion left
-- behind (edit history, edit timestamp, reply reference) is purged in Rust when
-- the client DB is opened, where a deletion is derived from the message content
-- instead of the status.
DELETE FROM reaction
WHERE target_mimi_id NOT IN (SELECT mimi_id FROM message WHERE mimi_id IS NOT NULL);
