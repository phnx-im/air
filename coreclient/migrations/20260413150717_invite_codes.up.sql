-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

CREATE TABLE invitation_code (
    code TEXT NOT NULL PRIMARY KEY,
    copied BOOLEAN NOT NULL DEFAULT FALSE
);
