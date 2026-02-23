-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
--
-- Add self_updated_at column to group table
ALTER TABLE "group"
ADD COLUMN self_updated_at TEXT;

CREATE INDEX IF NOT EXISTS idx_group_self_updated_at ON "group" (self_updated_at);
