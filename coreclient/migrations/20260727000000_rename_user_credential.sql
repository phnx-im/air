ALTER TABLE client_credential RENAME TO user_credential;
ALTER TABLE user_credential RENAME COLUMN client_credential TO user_credential;
DROP INDEX idx_client_credential_user_id;
CREATE INDEX idx_user_credential_user_id ON user_credential (user_uuid, user_domain);
