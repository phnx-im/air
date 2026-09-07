-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Move the attachment content blob into a dedicated 1:1 table.
--
-- This makes reading from the attachment table cheaper.
--
-- The data is moved by the paired code migration in small batches, so that
-- pages freed by clearing a blob are reused by the next batch and the
-- database file does not grow. The old column is dropped by the next
-- migration.
CREATE TABLE attachment_content (
    attachment_id BLOB NOT NULL PRIMARY KEY,
    content BLOB NOT NULL,
    FOREIGN KEY (attachment_id) REFERENCES attachment (attachment_id) ON DELETE CASCADE
);
