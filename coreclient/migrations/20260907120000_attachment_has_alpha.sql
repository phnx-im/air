-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later

-- Whether the attachment is an image with an alpha channel.
-- NULL = not yet classified.
ALTER TABLE attachment
ADD COLUMN has_alpha BOOLEAN;
