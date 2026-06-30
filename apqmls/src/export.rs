// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::{group::ExportGroupInfoError, prelude::OpenMlsCrypto};

use crate::{
    ApqMlsGroup,
    authentication::ApqSigner,
    messages::{ApqMlsMessageOut, ApqRatchetTree},
};

impl ApqMlsGroup {
    /// Export the ratchet tree of the group.
    pub fn export_ratchet_tree(&self) -> ApqRatchetTree {
        let t_ratchet_tree = self.t_group.export_ratchet_tree();
        let pq_ratchet_tree = self.pq_group.export_ratchet_tree();
        ApqRatchetTree {
            t_ratchet_tree,
            pq_ratchet_tree,
        }
    }

    /// Export the group info of the group.
    pub fn export_group_info(
        &self,
        crypto: &impl OpenMlsCrypto,
        signer: &impl ApqSigner,
        with_ratchet_tree: bool,
    ) -> Result<ApqMlsMessageOut, ExportGroupInfoError> {
        let t_group_info =
            self.t_group
                .export_group_info(crypto, signer.t_signer(), with_ratchet_tree)?;
        let pq_group_info =
            self.pq_group
                .export_group_info(crypto, signer.pq_signer(), with_ratchet_tree)?;
        let group_info = ApqMlsMessageOut {
            t_message: t_group_info,
            pq_message: pq_group_info,
        };
        Ok(group_info)
    }
}
