-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Restore privacypass columns on as_client_record and drop the operation-type table
DROP TABLE as_token_allowance;

ALTER TABLE as_client_record
    ADD COLUMN remaining_tokens INTEGER NOT NULL DEFAULT 10,
    ADD COLUMN allowance_epoch SMALLINT NOT NULL DEFAULT 0;

ALTER TABLE as_batched_key
    DROP COLUMN IF EXISTS operation_type;