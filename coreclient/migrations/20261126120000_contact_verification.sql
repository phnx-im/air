-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Users marked as verified, with the fingerprint of the credential and the
-- time of the mark.
CREATE TABLE contact_verification (
    user_uuid BLOB NOT NULL,
    user_domain TEXT NOT NULL,
    credential_fingerprint BLOB NOT NULL,
    verified_at TEXT NOT NULL,
    PRIMARY KEY (user_uuid, user_domain)
);

