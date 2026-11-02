// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    messages::{
        client_ds::{AadMessage, AadPayload},
        client_ds_out::DeleteGroupParamsOut,
    },
    time::TimeStamp,
};
use airprotos::client::{
    app_data::GroupAppData,
    component::{AIR_COMPONENT_ID, GroupTombstone},
};
use anyhow::{anyhow, ensure};
use apqmls::{authentication::ApqSigner, commit_builder::ApqCommitMessageBundle};
use mls_assist::messages::AssistedMessageOut;
use openmls::{
    component::ComponentData,
    prelude::{
        AppDataUpdateProposal, OpenMlsProvider, Proposal, tls_codec::Serialize as TlsSerializeTrait,
    },
};
use openmls_traits::signatures::Signer;
use tracing::warn;

use crate::{
    db::access::WriteConnection,
    groups::{Group, openmls_provider::AirOpenMlsProvider},
};

impl Group {
    /// Stages the commit that deletes the group. The commit removes every other
    /// member. If `deleted` is Some, the commit also sets a tombstone recording
    /// the deletion in the Air component of the group's context.
    pub(crate) async fn stage_delete(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &impl Signer,
        deleted: Option<TimeStamp>,
    ) -> anyhow::Result<DeleteGroupParamsOut> {
        let vc_group_id = self.resolve_vc_emulation_group(&mut connection).await?;
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let remove_indices = self
            .mls_group()
            .members()
            .filter_map(|m| {
                if m.index != self.mls_group().own_leaf_index() {
                    Some(m.index)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let component_data = self.deleted_air_component(deleted)?;

        // There shouldn't be a welcome
        let aad_payload = AadPayload::DeleteGroup;
        let aad = AadMessage::from(aad_payload).tls_serialize_detached()?;
        self.mls_group.set_aad(aad);

        let mut builder = self.mls_group.commit_builder().force_self_update(true);
        if let Some(group_id) = &vc_group_id {
            builder = builder.vc_emulation(provider.crypto(), provider.storage(), group_id)?;
        }
        let mut builder = builder.propose_removals(remove_indices);
        if let Some(component_data) = &component_data {
            builder = builder.add_proposal(Proposal::AppDataUpdate(Box::new(
                AppDataUpdateProposal::update(AIR_COMPONENT_ID, component_data.data().to_vec()),
            )));
        }
        let mut builder = builder.load_psks(provider.storage())?;
        if let Some(component_data) = component_data {
            let mut updater = builder.app_data_dictionary_updater();
            updater.set(component_data);
            builder.with_app_data_dictionary_updates(updater.changes());
        }
        let (mls_message, _welcome_option, group_info_option) = builder
            .create_group_info(true)
            .build(provider.rand(), provider.crypto(), signer, |_| true)?
            .stage_commit(&provider)?
            .into_contents();

        debug_assert!(_welcome_option.is_none());
        let group_info =
            group_info_option.ok_or(anyhow!("No group info after commit operation"))?;
        let commit = AssistedMessageOut::new(mls_message, Some(group_info.into()));

        let params = DeleteGroupParamsOut { commit };
        Ok(params)
    }

    /// The APQ counterpart of [`Self::stage_delete`]. The flag goes into the
    /// T group context only.
    pub(crate) async fn stage_apq_delete(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &impl ApqSigner,
        deleted: Option<TimeStamp>,
    ) -> anyhow::Result<ApqCommitMessageBundle> {
        let vc_group_id = self.resolve_vc_emulation_group(&mut connection).await?;
        let provider = AirOpenMlsProvider::new(connection.as_mut());

        let removed_indices = self
            .mls_group()
            .members()
            .filter_map(|m| {
                if m.index != self.mls_group().own_leaf_index() {
                    Some(m.index)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let component_data = self.deleted_air_component(deleted)?;

        let (t_group, pq_group) = self.apq_mls_groups_mut()?;

        let aad_payload = AadPayload::DeleteGroup;
        let aad = AadMessage::from(aad_payload).tls_serialize_detached()?;
        t_group.set_aad(aad);

        let mut builder = apqmls::commit_builder::CommitBuilder::from_groups(t_group, pq_group)
            .force_self_update(true)
            .propose_removals(removed_indices)
            .create_group_info(true);
        if let Some(component_data) = component_data {
            builder = builder.update_t_app_data(component_data);
        }
        if let Some(group_id) = vc_group_id {
            builder = builder.vc_emulation(group_id);
        }
        let bundle = builder.finalize(&provider, signer, |_| true, |_| true)?;
        debug_assert!(bundle.welcome.is_none());
        ensure!(
            bundle.group_info.is_some(),
            "No group info after commit operation"
        );

        Ok(bundle)
    }

    /// The Air component of the T group context with a tombstone marking the
    /// group as deleted at `deleted`, ready for an app data update. `None` if
    /// no tombstone is to be set.
    fn deleted_air_component(
        &self,
        deleted: Option<TimeStamp>,
    ) -> anyhow::Result<Option<ComponentData>> {
        let Some(deleted) = deleted else {
            return Ok(None);
        };
        let Some(mut component) = GroupAppData::air_component(self.mls_group.extensions()) else {
            warn!(
                group_id = ?self.group_id(),
                "Group context has no Air component, deleting without a tombstone"
            );
            return Ok(None);
        };
        component.tombstone = Some(GroupTombstone::deleted(deleted));
        Ok(Some(component.to_component_data()?))
    }
}
