-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Add last resort flag to connection package table
ALTER TABLE connection_package
ADD COLUMN is_last_resort BOOLEAN DEFAULT 0;
