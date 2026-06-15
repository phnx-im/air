-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Add pending_commit_failed column to group table
ALTER TABLE "group"
ADD COLUMN pending_commit_failed INTEGER NOT NULL DEFAULT 0;
