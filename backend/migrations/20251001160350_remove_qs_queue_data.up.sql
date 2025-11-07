-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
ALTER TABLE qs_queues
DROP CONSTRAINT qs_queues_queue_id_fkey;

DROP TABLE qs_queue_data;
