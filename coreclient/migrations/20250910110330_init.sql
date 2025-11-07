-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Notes:
--  * DateTime is stored as TEXT
--
-- MLS Storage Provider Schema
--
-- Notes:
--  * Tables in this schema are indepdent of other tables and cannot be
--    referenced in foreign key constraints.
--  * Indexes are prefixed with `idx_` to avoid name clashes with other
--    tables.
--
CREATE TABLE group_data (
    group_id BLOB NOT NULL,
    data_type TEXT NOT NULL CHECK (
        data_type IN (
            'join_group_config',
            'tree',
            'interim_transcript_hash',
            'context',
            'confirmation_tag',
            'group_state',
            'message_secrets',
            'resumption_psk_store',
            'own_leaf_index',
            'use_ratchet_tree_extension',
            'group_epoch_secrets'
        )
    ),
    group_data BLOB NOT NULL,
    PRIMARY KEY (group_id, data_type)
);

CREATE TABLE proposal (
    group_id BLOB NOT NULL,
    proposal_ref BLOB NOT NULL,
    proposal BLOB NOT NULL,
    PRIMARY KEY (group_id, proposal_ref)
);

CREATE TABLE own_leaf_node (
    group_id BLOB PRIMARY KEY,
    leaf_node BLOB NOT NULL
);

CREATE TABLE signature_key (
    public_key BLOB PRIMARY KEY,
    signature_key BLOB NOT NULL
);

CREATE TABLE encryption_key (
    public_key BLOB PRIMARY KEY,
    key_pair BLOB NOT NULL
);

CREATE TABLE epoch_key_pairs (
    group_id BLOB NOT NULL,
    epoch_id BLOB NOT NULL,
    leaf_index INTEGER NOT NULL,
    key_pairs BLOB NOT NULL,
    PRIMARY KEY (group_id, epoch_id, leaf_index)
);

CREATE TABLE key_package (
    key_package_ref BLOB PRIMARY KEY,
    key_package BLOB NOT NULL
);

CREATE TABLE psk (psk_id BLOB PRIMARY KEY, psk_bundle BLOB NOT NULL);

--
-- Client Records Schema
--
-- Notes:
--  * The `client_record` table is independent and cannot be referenced in
--    other tables.
CREATE TABLE client_record (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    record_state TEXT NOT NULL CHECK (record_state IN ('in_progress', 'finished')),
    created_at DATETIME NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (user_uuid, user_domain)
);

--
-- Client Storage Schema
--
CREATE TABLE user_creation_state (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    state BLOB NOT NULL,
    PRIMARY KEY (user_uuid, user_domain)
);

CREATE TABLE own_client_info (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    server_url TEXT NOT NULL,
    qs_user_id BLOB NOT NULL,
    qs_client_id BLOB NOT NULL,
    PRIMARY KEY (user_uuid, user_domain)
);

CREATE TABLE user (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    epoch INTEGER NOT NULL,
    decryption_key_index BLOB NOT NULL,
    display_name TEXT NOT NULL,
    profile_picture BLOB,
    PRIMARY KEY (user_uuid, user_domain)
);

CREATE TABLE "group" (
    group_id BLOB NOT NULL PRIMARY KEY,
    identity_link_wrapper_key BLOB NOT NULL,
    group_state_ear_key BLOB NOT NULL,
    pending_diff BLOB,
    room_state BLOB NOT NULL
);

CREATE TABLE client_credential (
    fingerprint BLOB NOT NULL PRIMARY KEY,
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    client_credential BLOB NOT NULL
);

CREATE INDEX idx_client_credential_user_id ON client_credential (user_uuid, user_domain);

CREATE TABLE group_membership (
    group_id BLOB NOT NULL,
    leaf_index INTEGER NOT NULL,
    status TEXT DEFAULT 'staged_update' NOT NULL CHECK (
        status IN (
            'staged_update',
            'staged_removal',
            'staged_add',
            'merged'
        )
    ),
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    PRIMARY KEY (group_id, leaf_index, status)
);

CREATE INDEX idx_group_membership_user_id ON group_membership (user_uuid, user_domain);

CREATE TRIGGER delete_orphaned_data AFTER DELETE ON group_membership FOR EACH ROW BEGIN
-- Delete user profiles of users that are not in any group and that are not our own.
DELETE FROM user
WHERE
    user_uuid = OLD.user_uuid
    AND user_domain = OLD.user_domain
    AND NOT EXISTS (
        SELECT
            1
        FROM
            group_membership
        WHERE
            user_uuid = OLD.user_uuid
            AND user_domain = OLD.user_domain
    )
    AND NOT EXISTS (
        SELECT
            1
        FROM
            own_client_info
        WHERE
            user_uuid = OLD.user_uuid
            AND user_domain = OLD.user_domain
    );

END;

CREATE TABLE indexed_key (
    key_index BLOB NOT NULL PRIMARY KEY,
    key_value BLOB NOT NULL,
    base_secret BLOB NOT NULL
);

CREATE TABLE own_key_index (
    key_type TEXT CHECK (key_type IN ('user_profile_key')) PRIMARY KEY,
    key_index BLOB NOT NULL,
    FOREIGN KEY (key_index) REFERENCES indexed_key (key_index) ON DELETE CASCADE
);

CREATE TABLE chat (
    chat_id BLOB NOT NULL PRIMARY KEY,
    chat_title TEXT NOT NULL,
    chat_picture BLOB,
    group_id BLOB NOT NULL,
    last_read TEXT NOT NULL,
    -- missing `connection_as_{client_uuid,domain}` fields means it is a group chat
    connection_user_uuid BLOB,
    connection_user_domain TEXT,
    is_confirmed_connection BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    connection_user_handle TEXT
);

CREATE INDEX idx_chat_connection_user ON chat (connection_user_uuid, connection_user_domain);

CREATE TABLE chat_past_member (
    chat_id BLOB NOT NULL,
    member_user_uuid BLOB NOT NULL,
    member_user_domain TEXT NOT NULL,
    PRIMARY KEY (chat_id, member_user_uuid, member_user_domain),
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);

CREATE INDEX idx_chat_past_member_chat_id ON chat_past_member (chat_id);

CREATE TABLE contact (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    chat_id BLOB NOT NULL,
    wai_ear_key BLOB NOT NULL,
    friendship_token BLOB NOT NULL,
    PRIMARY KEY (user_uuid, user_domain),
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);

CREATE INDEX idx_contact_chat_id ON contact (chat_id);

CREATE TABLE message (
    message_id BLOB NOT NULL PRIMARY KEY,
    chat_id BLOB NOT NULL,
    timestamp TEXT NOT NULL,
    -- missing `sender_as_{client_uuid,domain}` fields means it is a system message
    sender_user_uuid BLOB,
    sender_user_domain TEXT,
    content BLOB NOT NULL,
    sent BOOLEAN NOT NULL,
    mimi_id BLOB,
    status INT NOT NULL DEFAULT 0,
    edited_at TEXT,
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX idx_message_chat_id ON message (chat_id);

CREATE INDEX idx_message_timetstamp ON message (timestamp);

CREATE INDEX idx_message_mimi_id ON message (mimi_id);

CREATE TABLE qs_verifying_key (
    user_domain TEXT PRIMARY KEY,
    verifying_key BLOB NOT NULL
);

CREATE TABLE queue_ratchet (
    queue_type TEXT PRIMARY KEY CHECK (queue_type IN ('qs')),
    queue_ratchet BLOB NOT NULL,
    sequence_number INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE as_credential (
    fingerprint BLOB PRIMARY KEY,
    user_domain TEXT NOT NULL,
    credential_type TEXT NOT NULL CHECK (
        credential_type IN ('as_credential', 'as_intermediate_credential')
    ),
    credential BLOB NOT NULL
);

CREATE TABLE store_notification (
    entity_id BLOB NOT NULL,
    kind INTEGER NOT NULL,
    added BOOLEAN NOT NULL,
    updated BOOLEAN NOT NULL,
    removed BOOLEAN NOT NULL,
    PRIMARY KEY (entity_id, kind)
);

CREATE TRIGGER delete_keys AFTER DELETE ON user FOR EACH ROW BEGIN
DELETE FROM indexed_key
WHERE
    key_index = OLD.decryption_key_index;

END;

CREATE TABLE user_handle (
    handle TEXT NOT NULL PRIMARY KEY,
    hash BLOB NOT NULL,
    signing_key BLOB NOT NULL,
    created_at TEXT NOT NULL,
    refreshed_at TEXT NOT NULL
);

CREATE TABLE user_handle_contact (
    -- Not referencing the user_handle table, because we don't want to delete
    -- the contact when the user handle is deleted.
    user_handle TEXT NOT NULL PRIMARY KEY,
    -- 1:1 relationship with chat
    chat_id BLOB NOT NULL UNIQUE,
    friendship_package_ear_key BLOB NOT NULL,
    created_at TEXT NOT NULL,
    connection_offer_hash BLOB NOT NULL,
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE
);

CREATE TABLE attachment (
    attachment_id BLOB NOT NULL PRIMARY KEY,
    chat_id BLOB NOT NULL,
    message_id BLOB NOT NULL,
    content_type TEXT NOT NULL,
    content BLOB,
    status INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES message (message_id) ON DELETE CASCADE
);

CREATE INDEX idx_attachment_chat_id ON attachment (chat_id);

CREATE INDEX idx_attachment_created_at ON attachment (created_at);

CREATE TABLE pending_attachment (
    attachment_id BLOB NOT NULL PRIMARY KEY,
    size INTEGER NOT NULL,
    enc_alg INTEGER NOT NULL,
    enc_key BLOB NOT NULL,
    nonce BLOB NOT NULL,
    aad BLOB NOT NULL,
    hash_alg INTEGER NOT NULL,
    hash BLOB NOT NULL,
    FOREIGN KEY (attachment_id) REFERENCES attachment (attachment_id) ON DELETE CASCADE
);

CREATE TABLE user_setting (
    setting TEXT NOT NULL PRIMARY KEY,
    value BLOB NOT NULL
);

CREATE TABLE connection_package (
    connection_package_hash BLOB NOT NULL PRIMARY KEY,
    handle TEXT NOT NULL,
    decryption_key BLOB NOT NULL,
    expires_at TEXT NOT NULL,
    FOREIGN KEY (handle) REFERENCES user_handle (handle) ON DELETE CASCADE
);

CREATE TABLE message_draft (
    chat_id BLOB NOT NULL PRIMARY KEY,
    message TEXT NOT NULL,
    editing_id BLOB,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (chat_id) REFERENCES chat (chat_id) ON DELETE CASCADE,
    FOREIGN KEY (editing_id) REFERENCES message (message_id) ON DELETE CASCADE
);

CREATE TABLE message_status (
    message_id BLOB NOT NULL,
    sender_user_uuid BLOB NOT NULL,
    sender_user_domain TEXT NOT NULL,
    status INT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (message_id, sender_user_domain, sender_user_uuid),
    FOREIGN KEY (message_id) REFERENCES message (message_id) ON DELETE CASCADE
);

CREATE TABLE message_edit (
    mimi_id BLOB NOT NULL PRIMARY KEY,
    -- the message that was edited
    --
    -- The content of the message is always the latest version of the message.
    -- That is, the latest edit contains the previous message content. The
    -- second latest edit contains the content before the latest edit, and so
    -- on.
    message_id BLOB NOT NULL,
    created_at TEXT NOT NULL,
    -- content of the edited message
    content BLOB NOT NULL,
    FOREIGN KEY (message_id) REFERENCES message (message_id) ON DELETE CASCADE
);

CREATE INDEX idx_message_edit_message_id ON message_edit (message_id);
