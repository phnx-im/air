-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
CREATE TABLE message_draft_link_preview (
    chat_id BLOB NOT NULL PRIMARY KEY,
    url TEXT NOT NULL,
    card BLOB, -- encoded LinkPreviewCard, NULL = dismissed
    image BLOB,
    fetched_at TEXT,
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);
