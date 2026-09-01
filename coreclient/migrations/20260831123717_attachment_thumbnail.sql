-- Move the attachment content blob into a dedicated 1:1 table.
--
-- This makes reading from the attachment table cheaper.
CREATE TABLE attachment_content (
    attachment_id BLOB NOT NULL PRIMARY KEY,
    content BLOB NOT NULL,
    FOREIGN KEY (attachment_id) REFERENCES attachment (attachment_id) ON DELETE CASCADE
);

INSERT INTO
    attachment_content (attachment_id, content)
SELECT
    attachment_id,
    content
FROM
    attachment
WHERE
    content IS NOT NULL;

ALTER TABLE attachment
DROP COLUMN content;

-- Whether the attachment is an animated image.
-- NULL = not yet classified.
ALTER TABLE attachment
ADD COLUMN is_animated BOOLEAN;

-- Holds locally generated thumbnails for image attachments.
CREATE TABLE attachment_thumbnail (
    attachment_id BLOB NOT NULL PRIMARY KEY,
    state INTEGER NOT NULL, -- 1 ready, 2 original_fits, 3 failed
    created_at DATETIME NOT NULL,
    content BLOB, -- set iff state = ready
    FOREIGN KEY (attachment_id) REFERENCES attachment (attachment_id) ON DELETE CASCADE
);
