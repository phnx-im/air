-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Signing keys of unfinished username registrations.
--
CREATE TABLE pending_username(
    handle text NOT NULL PRIMARY KEY,
    signing_key BLOB NOT NULL
);

