-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

ALTER TABLE as_client_record DROP COLUMN allowance_epoch;
ALTER TABLE as_batched_key DROP COLUMN created_at;
