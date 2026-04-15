-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Drop privacypass columns on as_client_record and create a table
-- that let us categorise token quotas by operation type
ALTER TABLE as_client_record DROP COLUMN remaining_tokens, DROP COLUMN allowance_epoch;

CREATE TABLE as_token_allowance(
    user_uuid uuid NOT NULL,
    user_domain TEXT NOT NULL,
    operation_type SMALLINT NOT NULL,
    remaining SMALLINT NOT NULL,
    epoch SMALLINT NOT NULL,
    PRIMARY KEY (user_uuid, user_domain, operation_type),
    FOREIGN KEY (user_uuid, user_domain) REFERENCES as_user_record (user_uuid, user_domain) ON DELETE CASCADE,
    CONSTRAINT unique_user_operation UNIQUE (user_uuid, user_domain, operation_type)
);

-- we mark existing records as 1 (AddUsername) for backwards compatibility
ALTER TABLE as_token_nonce ADD COLUMN operation_type SMALLINT NOT NULL DEFAULT 1;
ALTER TABLE as_token_nonce ALTER COLUMN operation_type DROP DEFAULT;

ALTER TABLE as_batched_key ADD COLUMN operation_type SMALLINT NOT NULL DEFAULT 1;
ALTER TABLE as_batched_key ALTER COLUMN operation_type DROP DEFAULT;
