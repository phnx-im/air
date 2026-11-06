-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- The T and PQ epochs of the group state a resync must not go behind. NULL
-- until recorded. The PQ epoch stays NULL for T-only groups.
ALTER TABLE resync_queue ADD COLUMN baseline_t_epoch INTEGER;

ALTER TABLE resync_queue ADD COLUMN baseline_pq_epoch INTEGER;
