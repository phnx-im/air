-- SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE qs_queue_data (
    queue_id uuid PRIMARY KEY,
    sequence_number BIGINT NOT NULL,
    FOREIGN KEY (queue_id) REFERENCES qs_client_record (client_id) ON DELETE CASCADE
);

ALTER TABLE qs_queues
ADD CONSTRAINT qs_queues_queue_id_fkey FOREIGN KEY (queue_id) REFERENCES qs_queue_data (queue_id) ON DELETE CASCADE;
