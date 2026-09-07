-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Tag tokens with the batch they came from.
--
-- Tokens issued on the counter-metered path keep NULL in both columns and stay
-- spendable: FIFO consumption drains them first because they are the oldest
-- rows.
ALTER TABLE privacy_pass_token
ADD COLUMN allowance_epoch INTEGER;

ALTER TABLE privacy_pass_token
ADD COLUMN token_index INTEGER;

-- Re-derived tokens are byte-identical, so re-fetching a batch inserts nothing
-- instead of duplicating rows. Metered tokens are random and never collide.
CREATE UNIQUE INDEX privacy_pass_token_token ON privacy_pass_token (token);
