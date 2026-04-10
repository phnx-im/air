-- Drop privacypass columns on as_client_record and create a table
-- that let us categorise token quotas by operation type
ALTER TABLE as_client_record DROP COLUMN remaining_tokens, DROP COLUMN allowance_epoch;

CREATE TABLE as_token_allowance(
    user_uuid uuid PRIMARY KEY,
    user_domain TEXT NOT NULL,
    operation_type SMALLINT NOT NULL,
    remaining_tokens INTEGER NOT NULL,
    allowance_epoch SMALLINT NOT NULL,
    FOREIGN KEY (user_uuid, user_domain) REFERENCES as_user_record (user_uuid, user_domain) ON DELETE CASCADE
);
