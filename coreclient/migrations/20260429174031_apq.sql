-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
CREATE TABLE apq_key_package_refs (
    key_package_ref BLOB PRIMARY KEY,
    is_live INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_apq_key_package_ref_is_live ON apq_key_package_refs (is_live);
