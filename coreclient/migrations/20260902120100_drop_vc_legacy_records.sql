-- SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
--
-- SPDX-License-Identifier: AGPL-3.0-or-later
--
-- Dropped in their own migration, after the code migration paired with the
-- previous one has read them, so that the schema the checked queries compile
-- against never contains them.
DROP TABLE vc_registered_emulation_epoch;
DROP TABLE vc_emulation_binding_record;
