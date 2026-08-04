-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The contact behind a connection group a freshly linked device onboards into.
--
-- A connection chat needs a `contact` row next to its chat, but the friendship
-- token and WAI key are not recoverable from the group: they only ever travel
-- inside an HPKE-sealed friendship package. They arrive over the linking
-- channel, and have to survive until the onboarding external commit is
-- accepted, which is when the chat (and hence the contact, which references it)
-- can be created.
--
-- All four columns are set together or not at all. NULL means the resync
-- onboards into a group chat.
ALTER TABLE resync_queue ADD COLUMN connection_user_uuid BLOB;

ALTER TABLE resync_queue ADD COLUMN connection_user_domain TEXT;

ALTER TABLE resync_queue ADD COLUMN connection_wai_ear_key BLOB;

ALTER TABLE resync_queue ADD COLUMN connection_friendship_token BLOB;
