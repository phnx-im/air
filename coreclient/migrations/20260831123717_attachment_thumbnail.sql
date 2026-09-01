-- Holds locally generated thumbnails for image attachments.
CREATE TABLE attachment_thumbnail (
    attachment_id BLOB NOT NULL PRIMARY KEY,
    state INTEGER NOT NULL, -- 1 ready, 2 original_fits, 3 failed
    created_at DATETIME NOT NULL,
    content BLOB, -- set iff state = ready
    FOREIGN KEY (attachment_id) REFERENCES attachment (attachment_id) ON DELETE CASCADE
);

-- Whether the attachment is an animated image.
-- NULL = not yet classified.
ALTER TABLE attachment
ADD COLUMN is_animated BOOLEAN;
