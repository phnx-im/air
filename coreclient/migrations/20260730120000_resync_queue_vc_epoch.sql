-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The virtual-client emulation epoch a queued resync must be built against.
-- NULL for an ordinary resync. Set when the resync onboards an emulator client
-- into a higher-level group (variant B of the mls-virtual-clients draft): the
-- new leaf's key material is derived from this epoch's operation secret tree, so
-- sibling emulator clients can rederive it when they process the commit.
ALTER TABLE resync_queue
ADD COLUMN vc_epoch_id BLOB;
