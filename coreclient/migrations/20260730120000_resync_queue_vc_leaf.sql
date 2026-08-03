-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Whether the leaf this resync replaces is shared with sibling emulator clients
-- (variant B of the mls-virtual-clients draft).
ALTER TABLE resync_queue
ADD COLUMN shares_vc_leaf BOOLEAN NOT NULL DEFAULT FALSE;
