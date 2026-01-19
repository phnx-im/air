-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Remove the unused server_url column from the own_client_info table.
ALTER TABLE own_client_info
DROP COLUMN server_url;
