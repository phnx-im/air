// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{LeafCredential, keys::UserSigningKey},
    identifiers::UserId,
    messages::{
        client_ds::{AadMessage, AadPayload, GroupOperationParamsAad},
        client_ds_out::{ApqGroupOperationParamsOut, GroupOperationParamsOut},
    },
};
use anyhow::{Result, anyhow, ensure};
use mls_assist::messages::AssistedMessageOut;
use openmls::prelude::{OpenMlsProvider, tls_codec::Serialize as TlsSerializeTrait};
use uuid::Uuid;

use crate::{
    db::access::WriteConnection,
    groups::{Group, openmls_provider::AirOpenMlsProvider},
};

impl Group {
    pub(crate) async fn stage_remove(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &UserSigningKey,
        mut members: Vec<UserId>,
    ) -> Result<GroupOperationParamsOut> {
        // Note: The order of `remove_indices` is not the same as the order of `members`.
        let mut remove_indices = Vec::with_capacity(members.len());
        for member in self.mls_group.members() {
            let credential = LeafCredential::from_credential(&member.credential)?;
            let user_id = credential.user_id(self.own_user_id());
            if let Some(idx) = members.iter().position(|id| id == user_id) {
                remove_indices.push(member.index);
                members.swap_remove(idx);
            }
            if members.is_empty() {
                break;
            }
        }
        ensure!(members.is_empty(), "Not all members to remove were found");

        let aad_payload = AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys: vec![],
        });
        let aad = AadMessage::from(aad_payload).tls_serialize_detached()?;
        self.mls_group.set_aad(aad);
        let vc_group_id = self.resolve_vc_emulation_group(&mut connection).await?;
        let provider = AirOpenMlsProvider::new(connection.as_mut());

        let mut builder = self.mls_group.commit_builder().force_self_update(true);
        if let Some(group_id) = &vc_group_id {
            builder = builder.vc_emulation(provider.crypto(), provider.storage(), group_id)?;
        }
        let (mls_message, _welcome_option, group_info_option) = builder
            .propose_removals(remove_indices)
            .load_psks(provider.storage())?
            .create_group_info(true)
            .build(provider.rand(), provider.crypto(), signer, |_| true)?
            .stage_commit(&provider)?
            .into_contents();

        // There shouldn't be a welcome
        debug_assert!(_welcome_option.is_none());
        let group_info = group_info_option.ok_or(anyhow!("No group info after commit"))?;
        let commit = AssistedMessageOut::new(mls_message, Some(group_info.into()));

        let params = GroupOperationParamsOut {
            commit,
            add_users_info_option: None,
        };
        Ok(params)
    }

    /// The client ids of all self-group leaves this group can parse.
    ///
    /// Infallible and in-memory, for refining a pending removal against the
    /// current members. A leaf that fails to parse is skipped rather than
    /// erroring, unlike [`SelfGroup::client_ids`], which is the authoritative
    /// read used for display and for the unlink precondition.
    ///
    /// [`SelfGroup::client_ids`]: self_group::SelfGroup::client_ids
    pub(crate) fn self_group_client_ids(&self) -> Vec<Uuid> {
        self.mls_group
            .members()
            .filter_map(|member| {
                match LeafCredential::from_credential(&member.credential).ok()? {
                    LeafCredential::SelfGroup(credential) => Some(credential.client_id()),
                    LeafCredential::User(_) => None,
                }
            })
            .collect()
    }

    pub(crate) async fn stage_apq_remove(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &UserSigningKey,
        mut members: Vec<UserId>,
    ) -> anyhow::Result<ApqGroupOperationParamsOut> {
        // Note: The order of `remove_indices` is not the same as the order of `members`.
        let mut remove_indices = Vec::with_capacity(members.len());
        for member in self.mls_group.members() {
            let credential = LeafCredential::from_credential(&member.credential)?;
            let user_id = credential.user_id(self.own_user_id());
            if let Some(idx) = members.iter().position(|id| id == user_id) {
                remove_indices.push(member.index);
                members.swap_remove(idx);
            }
            if members.is_empty() {
                break;
            }
        }
        ensure!(members.is_empty(), "Not all members to remove were found");

        let vc_group_id = self.resolve_vc_emulation_group(&mut connection).await?;
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let (t_mls_group, pq_mls_group) = self.apq_mls_groups_mut()?;

        let aad_payload = AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys: Vec::new(),
        });
        let aad = AadMessage::from(aad_payload).tls_serialize_detached()?;
        t_mls_group.set_aad(aad);

        let mut builder =
            apqmls::commit_builder::CommitBuilder::from_groups(t_mls_group, pq_mls_group)
                .force_self_update(true)
                .propose_removals(remove_indices)
                .create_group_info(true);
        if let Some(group_id) = vc_group_id {
            builder = builder.vc_emulation(group_id);
        }
        let bundle = builder.finalize(&provider, signer, |_| true, |_| true)?;

        debug_assert!(bundle.welcome.is_none());
        ensure!(
            bundle.group_info.is_some(),
            "No group info in APQMLS bundle"
        );

        Ok(ApqGroupOperationParamsOut {
            bundle,
            encrypted_welcome_attribution_infos: Vec::new(),
        })
    }
}
