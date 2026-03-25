-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
--
-- Speeds up finding attachments that have not yet been downloaded
CREATE INDEX IF NOT EXISTS idx_attachment_pending_ordered ON attachment (created_at)
WHERE
    status = 1;
