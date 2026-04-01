-- SPDX-FileCopyrightText: 202 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
ALTER TABLE invitation_code 
DROP CONSTRAINT IF EXISTS fk_invitation_user,
DROP COLUMN IF EXISTS user_uuid,
DROP COLUMN IF EXISTS user_domain,
DROP COLUMN IF EXISTS created_at;