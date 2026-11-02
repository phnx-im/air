// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{UserCredential, keys::UserSigningKey},
    crypto::{
        aead::{AeadEncryptable, keys::WelcomeAttributionInfoEarKey},
        signatures::signable::Signable,
    },
    messages::{
        client_ds::{AadMessage, AadPayload, GroupOperationParamsAad},
        client_ds_out::{AddUsersInfoOut, ApqGroupOperationParamsOut, GroupOperationParamsOut},
        welcome_attribution_info::{WelcomeAttributionInfoPayload, WelcomeAttributionInfoTbs},
    },
};
use anyhow::{Context, Result, bail, ensure};
use apqmls::authentication::ApqSigner;
use mls_assist::messages::AssistedMessageOut;
use openmls::{
    prelude::{
        MlsMessageOut, OpenMlsProvider, Proposal, ProtocolVersion,
        tls_codec::Serialize as TlsSerializeTrait,
    },
    treesync::errors::LeafNodeValidationError,
};

use crate::{
    contacts::{ContactAddInfos, ContactKeyPackage},
    db::access::WriteConnection,
    groups::{Group, commit::to_capabilities_mismatch, openmls_provider::AirOpenMlsProvider},
};

/// One contact that has been prepared for inviting to a group.
pub(crate) struct PreparedInvitee {
    pub(crate) add_info: ContactAddInfos,
    pub(crate) wai_key: WelcomeAttributionInfoEarKey,
    pub(crate) user_credential: UserCredential,
}

impl Group {
    /// Invite the given list of contacts to join the group.
    ///
    /// Returns the [`GroupOperationParamsOut`] as input for the pending chat operation processing.
    pub(crate) async fn stage_invite(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &UserSigningKey,
        invitees: Vec<PreparedInvitee>,
    ) -> anyhow::Result<Result<GroupOperationParamsOut, LeafNodeValidationError>> {
        debug_assert!(!self.is_apq(), "APQ group in non-APQ stage_invite");
        // Prepare KeyPackages

        let mut key_packages = Vec::with_capacity(invitees.len());
        let mut wai_keys = Vec::with_capacity(invitees.len());
        let mut new_encrypted_user_profile_keys = Vec::with_capacity(invitees.len());
        for PreparedInvitee {
            add_info,
            wai_key,
            user_credential,
        } in invitees
        {
            new_encrypted_user_profile_keys.push(
                add_info
                    .user_profile_key
                    .encrypt(&self.identity_link_wrapper_key, user_credential.user_id())?,
            );
            key_packages.push(add_info.key_package);
            wai_keys.push(wai_key);
        }

        let aad_message: AadMessage = AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys,
        })
        .into();

        // Set Aad to contain the encrypted user credentials.
        let key_packages = key_packages
            .into_iter()
            .map(|kp| match kp {
                ContactKeyPackage::Traditional(kp) => Ok(*kp),
                ContactKeyPackage::Apq(_) => {
                    bail!("APQ key package used for traditional group invite")
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let vc_group_id = self.resolve_vc_emulation_group(&mut connection).await?;

        let (mls_commit, welcome_option, group_info_option) = {
            let provider = AirOpenMlsProvider::new(connection.as_mut());
            self.mls_group
                .set_aad(aad_message.tls_serialize_detached()?);
            let mut builder = self.mls_group.commit_builder().force_self_update(true);
            if let Some(group_id) = &vc_group_id {
                builder = builder.vc_emulation(provider.crypto(), provider.storage(), group_id)?;
            }
            let res = builder
                .propose_adds(key_packages)
                .load_psks(provider.storage())?
                .create_group_info(true)
                .build(provider.rand(), provider.crypto(), signer, |_| true);
            match res {
                Ok(builder) => builder.stage_commit(&provider)?.into_contents(),
                // Extract leaf node validation error if any
                Err(error) => return Ok(Err(to_capabilities_mismatch(error)?)),
            }
        };

        let group_info = group_info_option.context("No group info found")?;
        let welcome = MlsMessageOut::from_welcome(
            welcome_option.context("No welcome message found")?,
            ProtocolVersion::default(),
        );
        let commit = AssistedMessageOut::new(mls_commit, Some(group_info.into()));

        let encrypted_welcome_attribution_infos = wai_keys
            .iter()
            .map(|wai_key| {
                // WAI = WelcomeAttributionInfo
                let wai_payload = WelcomeAttributionInfoPayload::new(
                    signer.credential().user_id().clone(),
                    self.identity_link_wrapper_key.clone(),
                );

                let wai = WelcomeAttributionInfoTbs {
                    payload: wai_payload,
                    group_id: self.group_id().clone(),
                    welcome: welcome.tls_serialize_detached()?,
                }
                .sign(signer)?;
                Ok(wai.encrypt(wai_key)?)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let add_users_info = AddUsersInfoOut {
            welcome,
            encrypted_welcome_attribution_infos,
        };

        let params = GroupOperationParamsOut {
            commit,
            add_users_info_option: Some(add_users_info),
        };

        Ok(Ok(params))
    }

    /// Invite the given list of contacts to join the APQ group.
    ///
    /// Returns the [`ApqGroupOperationParamsOut`] as input for the pending chat operation
    /// processing.
    ///
    /// `signer` signs the MLS commit (i.e. the committer's leaf), while
    /// `wai_signer` signs the WelcomeAttributionInfo. They differ only for the
    /// self group, where the leaf is signed with a fresh key but the WAI must be
    /// signed with the real user credential key so the joiner can verify it
    /// against the sender's user credential.
    ///
    /// `app_ephemeral` rides along on the same commit when set.
    pub(crate) async fn stage_apq_invite(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &impl ApqSigner,
        wai_signer: &UserSigningKey,
        invitees: Vec<PreparedInvitee>,
        app_ephemeral: Option<Proposal>,
    ) -> anyhow::Result<Result<ApqGroupOperationParamsOut, LeafNodeValidationError>> {
        debug_assert!(self.is_apq(), "Non-APQ group in APQ stage_invite");
        // Prepare KeyPackages

        let mut key_packages = Vec::with_capacity(invitees.len());
        let mut wai_keys = Vec::with_capacity(invitees.len());
        let mut new_encrypted_user_profile_keys = Vec::with_capacity(invitees.len());
        for PreparedInvitee {
            add_info,
            wai_key,
            user_credential,
        } in invitees
        {
            new_encrypted_user_profile_keys.push(
                add_info
                    .user_profile_key
                    .encrypt(&self.identity_link_wrapper_key, user_credential.user_id())?,
            );
            key_packages.push(add_info.key_package);
            wai_keys.push(wai_key);
        }

        let aad_message: AadMessage = AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys,
        })
        .into();

        let key_packages = key_packages
            .into_iter()
            .map(|kp| match kp {
                ContactKeyPackage::Traditional(_) => {
                    bail!("Traditional key package used for APQ group invite")
                }
                ContactKeyPackage::Apq(kp) => Ok(*kp),
            })
            .collect::<Result<Vec<_>>>()?;

        let vc_group_id = self.resolve_vc_emulation_group(&mut connection).await?;

        let provider = AirOpenMlsProvider::new(connection.as_mut());

        self.mls_group
            .set_aad(aad_message.tls_serialize_detached()?);

        let (t_mls_group, pq_mls_group) = self.apq_mls_groups_mut()?;
        let mut builder =
            apqmls::commit_builder::CommitBuilder::from_groups(t_mls_group, pq_mls_group)
                .force_self_update(true)
                .propose_adds(key_packages)
                .create_group_info(true);
        if let Some(proposal) = app_ephemeral {
            builder = builder.add_t_proposal(proposal);
        }
        if let Some(group_id) = vc_group_id {
            builder = builder.vc_emulation(group_id);
        }
        let bundle = match builder.finalize(&provider, signer, |_| true, |_| true) {
            Ok(bundle) => bundle,
            // Extract leaf node validation error if any
            Err(apqmls::commit_builder::CreateCommitError::BuildCommit(error)) => {
                return Ok(Err(to_capabilities_mismatch(error)?));
            }
            Err(other) => return Err(other.into()),
        };

        ensure!(
            bundle.group_info.is_some(),
            "No group info in APQMLS bundle"
        );

        let serialized_welcome = bundle
            .welcome
            .as_ref()
            .context("No welcome in APQMLS bundle")?
            .tls_serialize_detached()?;

        let encrypted_welcome_attribution_infos = wai_keys
            .iter()
            .map(|wai_key| {
                let wai_payload = WelcomeAttributionInfoPayload::new(
                    wai_signer.credential().user_id().clone(),
                    self.identity_link_wrapper_key.clone(),
                );
                let wai = WelcomeAttributionInfoTbs {
                    payload: wai_payload,
                    group_id: self.group_id().clone(),
                    welcome: serialized_welcome.clone(),
                }
                .sign(wai_signer)?;
                Ok(wai.encrypt(wai_key)?)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let params = ApqGroupOperationParamsOut {
            bundle,
            encrypted_welcome_attribution_infos,
        };

        Ok(Ok(params))
    }
}
