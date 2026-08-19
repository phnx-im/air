-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Token seed of a (operation type, VOPRF key). The token request of an
-- allowance epoch is derived from it, which is what makes issuance idempotent.
-- The seed never leaves the device except to the user's own other devices, over
-- the self group.
--
-- Set once per key: a changed seed derives a different request, and the AS only
-- answers the request it already locked for the allowance epoch.
CREATE TABLE privacy_pass_seed (
    operation_type INTEGER NOT NULL,
    key_fingerprint BLOB NOT NULL,
    seed BLOB NOT NULL,
    -- 'proposed' while the seed is staged for a self-group commit, 'committed'
    -- once the devices are known to agree on it. Only committed seeds derive
    -- token requests: a proposal that loses the commit race is replaced by the
    -- winner, and issuing from it would lock the allowance epoch to a request
    -- the siblings never send.
    state TEXT NOT NULL CHECK (state IN ('proposed', 'committed')),
    -- Set while the seed still has to go out on a self-group commit: a fresh
    -- proposal, or a committed seed that won a divergence and is re-broadcast
    -- so the sibling holding the higher seed converges on it.
    needs_broadcast BOOLEAN NOT NULL,
    created_at DATETIME NOT NULL,
    PRIMARY KEY (operation_type, key_fingerprint)
);
