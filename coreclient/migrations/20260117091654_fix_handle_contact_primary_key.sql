-- Recreate user_handle_contact as username_contact with chat_id as primary key
-- This allows multiple senders to use the same username without overwriting each other's contact requests
CREATE TABLE username_contact (
    chat_id BLOB NOT NULL PRIMARY KEY,
    username TEXT NOT NULL,
    friendship_package_ear_key BLOB NOT NULL,
    created_at TEXT NOT NULL,
    connection_offer_hash BLOB NOT NULL,
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);

-- Copy existing data
INSERT INTO username_contact
SELECT chat_id, user_handle, friendship_package_ear_key, created_at, connection_offer_hash
FROM user_handle_contact;

-- Drop old table
DROP TABLE user_handle_contact;

-- Add index for username lookups
CREATE INDEX idx_username_contact_username ON username_contact (username);
