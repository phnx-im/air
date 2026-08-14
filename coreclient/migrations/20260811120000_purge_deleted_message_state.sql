-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Clean up after messages deleted before a deletion cleaned up after itself.
--
-- A reaction is only ever stored for a message we have, so a target matching no
-- message is one whose Mimi ID an edit or a deletion rewrote. A target still
-- found in message_edit remains resolvable through the edit history, so those
-- reactions are kept: for a live message they target superseded versions of an
-- edit, and for a deletion that left its history behind the open-time purge in
-- Rust sweeps them through that history. A target in neither table is a
-- deletion leftover. Keying on the dangling reference rather than on the
-- deleted status matters because a peer's status report may have overwritten
-- that status. The remaining per-row state a deletion left behind (edit
-- history, edit timestamp, reply reference) is purged by the same open-time
-- pass, which derives a deletion from the message content instead of the
-- status.
DELETE FROM reaction
WHERE target_mimi_id NOT IN (SELECT mimi_id FROM message WHERE mimi_id IS NOT NULL)
    AND target_mimi_id NOT IN (SELECT mimi_id FROM message_edit);
