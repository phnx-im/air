-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Persist the last known push token so we can detect changes and enqueue updates.
CREATE TABLE push_token_state(
    id integer PRIMARY KEY CHECK (id = 1),
    operator INTEGER,
    token text,
    updated_at text NOT NULL,
    pending_update text
);
