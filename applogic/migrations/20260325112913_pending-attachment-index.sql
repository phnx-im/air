-- Speeds up finding attachments that have not yet been downloaded
CREATE INDEX idx_attachment_pending_ordered ON attachment (created_at)
WHERE
    status = 1;