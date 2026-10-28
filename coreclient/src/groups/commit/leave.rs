// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::client_ds_out::SelfRemoveParamsOut;
use anyhow::Result;
use mls_assist::messages::AssistedMessageOut;
use openmls::prelude::{OpenMlsProvider, QueuedProposal};
use openmls_traits::signatures::Signer;

use crate::{
    db::access::WriteConnection,
    groups::{Group, openmls_provider::AirOpenMlsProvider},
};

impl Group {
    pub(crate) fn stage_leave_group(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &impl Signer,
    ) -> Result<SelfRemoveParamsOut> {
        let provider = &AirOpenMlsProvider::new(connection.as_mut());

        let t_proposal = self
            .mls_group
            .leave_group_via_self_remove(provider, signer)?;
        let pq_proposal = self
            .pq_mut()
            .map(|pq| pq.mls_group.leave_group_via_self_remove(provider, signer))
            .transpose()?;

        let t_remove_proposal = AssistedMessageOut::new(t_proposal, None);
        let pq_remove_proposal =
            pq_proposal.map(|proposal| AssistedMessageOut::new(proposal, None));

        let params = SelfRemoveParamsOut {
            t_remove_proposal,
            pq_remove_proposal,
        };
        Ok(params)
    }

    pub(crate) fn restage_leave_group(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &impl Signer,
        existing: &SelfRemoveParamsOut,
    ) -> Result<SelfRemoveParamsOut> {
        let provider = &AirOpenMlsProvider::new(connection.as_mut());

        // T side: regenerate only if the T epoch drifted
        let t_remove_proposal =
            if existing.t_remove_proposal.epoch() == Some(self.mls_group().epoch()) {
                existing.t_remove_proposal.clone()
            } else {
                let proposal = self
                    .mls_group
                    .leave_group_via_self_remove(provider, signer)?;
                AssistedMessageOut::new(proposal, None)
            };

        // PQ side: regenerate only if the PQ epoch drifted (a FULL commit landed)
        let pq_remove_proposal = match self.pq_mut() {
            Some(pq) => {
                let pq_epoch = pq.mls_group.epoch();
                let existing_pq = existing.pq_remove_proposal.as_ref();
                if existing_pq.is_some_and(|proposal| proposal.epoch() == Some(pq_epoch)) {
                    existing_pq.cloned()
                } else {
                    let proposal = pq.mls_group.leave_group_via_self_remove(provider, signer)?;
                    Some(AssistedMessageOut::new(proposal, None))
                }
            }
            None => None,
        };

        Ok(SelfRemoveParamsOut {
            t_remove_proposal,
            pq_remove_proposal,
        })
    }

    pub(crate) fn store_proposal(
        &mut self,
        mut connection: impl WriteConnection,
        proposal: QueuedProposal,
    ) -> Result<()> {
        let provider = &AirOpenMlsProvider::new(connection.as_mut());
        self.mls_group
            .store_pending_proposal(provider.storage(), proposal)?;
        Ok(())
    }
}
