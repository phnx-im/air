-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

ALTER TABLE privacy_pass_token ADD COLUMN created_at DATETIME NOT NULL DEFAULT current_timestamp;

ALTER TABLE invitation_code ADD COLUMN created_at DATETIME NOT NULL DEFAULT current_timestamp;
