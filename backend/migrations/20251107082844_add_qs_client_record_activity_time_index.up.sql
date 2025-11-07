-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE INDEX idx_qs_client_record_user_activity ON qs_client_record (user_id, activity_time DESC);
