-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

ALTER TABLE batched_token_key DROP COLUMN operation_type;
ALTER TABLE privacy_pass_token DROP COLUMN operation_type;