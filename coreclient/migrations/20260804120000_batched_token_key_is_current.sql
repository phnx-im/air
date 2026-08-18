-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Record which cached VOPRF key the AS advertises as the current one.
--
-- Truncated key IDs are a hash of the public key, so they say nothing about a
-- key's age. Without this marker a client inside the rotation overlap window
-- can mint tokens under the outgoing key, which dies a few days later.
--
-- Existing rows default to not current, which reproduces the previous
-- behaviour (any key) until the next key fetch fills the marker in.
ALTER TABLE batched_token_key
ADD COLUMN is_current BOOLEAN NOT NULL DEFAULT FALSE;
