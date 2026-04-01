-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

ALTER TABLE invitation_code
DROP CONSTRAINT IF EXISTS fk_invitation_user,
ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
ADD COLUMN IF NOT EXISTS user_uuid UUID,
ADD COLUMN IF NOT EXISTS user_domain TEXT,
ADD CONSTRAINT fk_invitation_user 
    FOREIGN KEY (user_uuid, user_domain) 
    REFERENCES as_user_record (user_uuid, user_domain)
    ON DELETE CASCADE;