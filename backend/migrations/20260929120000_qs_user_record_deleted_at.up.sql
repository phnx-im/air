-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Deleted users keep their record as a tombstone, so that their remaining
-- clients can still read their queues.
--
ALTER TABLE qs_user_record
ADD COLUMN deleted_at TIMESTAMPTZ NULL;
