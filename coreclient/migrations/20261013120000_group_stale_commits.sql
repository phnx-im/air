-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Count consecutive stale rejections of our commits per group
ALTER TABLE "group"
ADD COLUMN stale_commits INTEGER NOT NULL DEFAULT 0;
