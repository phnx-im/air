// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::{LeafSigningKey, UserSigningKey},
    crypto::aead::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    identifiers::UserId,
    messages::{
        client_as::ConnectionOfferHash,
        client_ds::AadMessage,
        client_ds_out::{ExternalCommitInfoIn, PqExternalCommitInfoIn},
    },
    mls_group_config::{
        QS_CLIENT_REFERENCE_EXTENSION_TYPE, default_leaf_node_capabilities,
        default_mls_group_join_config, self_group_leaf_node_capabilities,
    },
};
use airprotos::client::app_data::{ClientAppData, GroupAppData};
use anyhow::{Context, Result, bail, ensure};
use apqmls::{
    authentication::ApqCredentialWithKey,
    commit_builder::ApqCommitMessageBundle,
    external_commit_builder::{ApqExternalCommitBuilder, ApqExternalCommitBuilderError},
    messages::{ApqProposalIn, ApqRatchetTreeIn, VerifiableApqGroupInfo},
};
use openmls::{
    group::ExternalCommitBuilder,
    prelude::{
        Credential, CredentialType, CredentialWithKey, Extension, GroupId, LeafNodeParameters,
        MlsMessageBodyIn, MlsMessageIn, MlsMessageOut, OpenMlsProvider, PreSharedKeyProposal,
        SignaturePublicKey, tls_codec::Serialize as TlsSerializeTrait,
    },
    treesync::{RatchetTreeIn, errors::LeafNodeValidationError},
};
use tls_codec::DeserializeBytes;

use crate::{
    clients::{api_clients::ApiClients, own_client_info::OwnClientInfo},
    db::access::WriteDbTransaction,
    groups::{
        Group,
        commit::to_capabilities_mismatch,
        connection_psk::store_connection_offer_psk,
        credentials::{verify_member_credentials, verify_pq_signature_keys},
        error::GroupClosed,
        join::{
            DecryptedProfileInfos, closer, ensure_self_group_flag, group_info_signer_credential,
        },
        openmls_provider::AirOpenMlsProvider,
        room_state::{add_joiner_to_room_state, ensure_room_state_matches_members},
    },
};

/// What an external join into a connection group needs beyond the group itself.
pub(crate) struct ConnectionGroupJoin<'a> {
    /// The sole member of the group, whose invitation admits us to its room state.
    pub(crate) inviter: &'a UserId,
    /// Set when the join answers a connection offer, `None` for the from-group flow.
    pub(crate) connection_offer_hash: Option<ConnectionOfferHash>,
}

impl Group {
    /// Join a group using an external commit.
    #[expect(clippy::too_many_arguments)]
    pub(crate) async fn join_group_externally(
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        external_commit_info: ExternalCommitInfoIn,
        signer: &UserSigningKey,
        group_state_ear_key: GroupStateEarKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        aad: AadMessage,
        // Should be Some if we are joining a connection group.
        connection_group: Option<ConnectionGroupJoin<'_>>,
        // Should be Some if we act as a virtual client emulated from that self
        // group, whether joining anew or resyncing an existing membership.
        vc_group_id: Option<GroupId>,
    ) -> anyhow::Result<
        Result<
            (Self, MlsMessageOut, MlsMessageOut, DecryptedProfileInfos),
            LeafNodeValidationError,
        >,
    > {
        let mls_group_config = default_mls_group_join_config();
        let credential_with_key = CredentialWithKey {
            credential: signer.credential().try_into()?,
            signature_key: signer.credential().verifying_key().clone().into(),
        };
        let ExternalCommitInfoIn {
            verifiable_group_info,
            ratchet_tree_in,
            encrypted_user_profile_keys,
            indexed_encrypted_user_profile_keys,
            mut room_state,
            proposals,
            pq,
        } = external_commit_info;

        ensure!(pq.is_none(), "APQ group in non-APQ stage_invite");
        let group_info_signer =
            group_info_signer_credential(&verifiable_group_info, &ratchet_tree_in);

        let proposals: Vec<_> = proposals
            .iter()
            .filter_map(|b| {
                let mls_message = MlsMessageIn::tls_deserialize_exact_bytes(b);
                let MlsMessageBodyIn::PublicMessage(pm) = mls_message.ok()?.extract() else {
                    return None;
                };
                Some(pm)
            })
            .collect();

        // Let's create the group first so that we can access the GroupId.
        // Phase 1: Create and store the group
        let (mls_group, commit, group_info, encrypted_profile_keys_fallback) = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            // Prepare PSK proposal if we have a connection offer hash.
            let psk_proposal = connection_group
                .as_ref()
                .and_then(|join| join.connection_offer_hash)
                .map(|co_hash| {
                    store_connection_offer_psk(
                        &provider,
                        verifiable_group_info.ciphersuite(),
                        co_hash,
                    )
                    .map(PreSharedKeyProposal::new)
                })
                .transpose()?;

            let leaf_node_extensions =
                ClientAppData::current_for(vc_group_id.is_some()).leaf_node_extensions();
            let leaf_node_parameters = LeafNodeParameters::builder()
                .with_capabilities(default_leaf_node_capabilities())
                .with_extensions(leaf_node_extensions)
                .build();

            let encrypted_profile_keys_fallback = Self::encrypted_profile_keys_fallback(
                &ratchet_tree_in,
                encrypted_user_profile_keys,
                &indexed_encrypted_user_profile_keys,
                signer.credential().user_id(),
            );

            let mut builder = ExternalCommitBuilder::new()
                .with_proposals(proposals)
                .with_aad(aad.tls_serialize_detached()?)
                .with_config(mls_group_config)
                .skip_lifetime_validation()
                .with_ratchet_tree(ratchet_tree_in)
                .build_group(&provider, verifiable_group_info, credential_with_key)?
                .leaf_node_parameters(leaf_node_parameters);

            // Must come after `leaf_node_parameters`: the VC leaf configuration
            // is validated against them before an operation secret is spent.
            if let Some(group_id) = &vc_group_id {
                builder = builder.vc_emulation(provider.crypto(), provider.storage(), group_id)?;
            }

            if let Some(psk_proposal) = psk_proposal {
                builder = builder.add_psk_proposal(psk_proposal);
            }

            let res = builder
                .load_psks(provider.storage())?
                .create_group_info(true)
                .build(provider.rand(), provider.crypto(), signer, |_| true);
            let (mls_group, commit) = match res {
                Ok(builder) => builder.finalize(&provider)?,
                // Extract leaf node validation error if any
                Err(error) => return Ok(Err(to_capabilities_mismatch(error)?)),
            };

            let (commit, _, group_info) = commit.into_contents();

            (
                mls_group,
                commit,
                group_info.context("No group info found")?,
                encrypted_profile_keys_fallback,
            )
        };

        let is_self_group =
            OwnClientInfo::is_own_self_group(&mut *txn, mls_group.group_id()).await?;
        ensure_self_group_flag(&mls_group, None, is_self_group)?;

        // Phase 3: Verify the user credentials. Self-group leaves carry a self-group credential,
        // which is only accepted inside our own self group.
        let credentials =
            verify_member_credentials(&mut *txn, api_clients, &mls_group, is_self_group).await?;
        if let Some(tombstone) = GroupAppData::tombstone(mls_group.extensions()) {
            let closer = closer(group_info_signer.as_ref(), signer.credential().user_id())?;
            bail!(GroupClosed { closer, tombstone });
        }
        if let Some(join) = connection_group {
            add_joiner_to_room_state(&mut room_state, join.inviter, signer.credential().user_id())?;
        }
        ensure_room_state_matches_members(&room_state, &mls_group)?;

        let group = Self::new(
            identity_link_wrapper_key,
            group_state_ear_key,
            mls_group,
            None,
            room_state,
            signer.credential().user_id().clone(),
        );

        // Phase 4: Store the group and client auth info, replacing any prior group with the same
        // id.
        Group::delete_from_db(txn, group.group_id()).await?;
        let member_profile_info = group
            .store_after_join(
                txn,
                credentials,
                indexed_encrypted_user_profile_keys,
                encrypted_profile_keys_fallback,
            )
            .await?;

        Ok(Ok((group, commit, group_info.into(), member_profile_info)))
    }

    /// Join an APQ group using an external commit.
    #[expect(clippy::too_many_arguments)]
    pub(crate) async fn join_apq_group_externally(
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        external_commit_info: ExternalCommitInfoIn,
        signer: &LeafSigningKey,
        own_user_id: &UserId,
        group_state_ear_key: GroupStateEarKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        aad: AadMessage,
        vc_group_id: Option<GroupId>,
        connection_group: Option<ConnectionGroupJoin<'_>>,
    ) -> anyhow::Result<
        Result<(Self, ApqCommitMessageBundle, DecryptedProfileInfos), LeafNodeValidationError>,
    > {
        // Prepare credentials. The leaf signature key is the signer's own key.
        let t_credential = CredentialWithKey {
            credential: signer.mls_credential()?,
            signature_key: signer.verifying_key().clone().into(),
        };
        // Skip storing the same credential twice
        let pq_credential = CredentialWithKey {
            credential: Credential::new(CredentialType::Basic, Vec::new()),
            signature_key: signer.verifying_key().clone().into(),
        };
        let credential_with_key = ApqCredentialWithKey {
            t_credential,
            pq_credential,
        };

        // Unpack the external commit info
        let ExternalCommitInfoIn {
            verifiable_group_info: t_group_info,
            ratchet_tree_in: t_ratchet_tree,
            encrypted_user_profile_keys,
            indexed_encrypted_user_profile_keys,
            mut room_state,
            proposals: t_proposals,
            pq:
                Some(PqExternalCommitInfoIn {
                    group_info: pq_group_info,
                    ratchet_tree: pq_ratchet_tree,
                    proposals: pq_proposals,
                }),
        } = external_commit_info
        else {
            bail!("Non-APQ group in APQ join");
        };

        ensure!(
            t_proposals.len() == pq_proposals.len(),
            "Invalid number of proposals"
        );
        let proposals: Vec<ApqProposalIn> = t_proposals
            .into_iter()
            .zip(pq_proposals)
            .filter_map(|(t, pq)| {
                // Invalid proposals are filtered out
                let t_mls_message = MlsMessageIn::tls_deserialize_exact_bytes(&t).ok()?;
                let pq_mls_message = MlsMessageIn::tls_deserialize_exact_bytes(&pq).ok()?;
                match (t_mls_message.extract(), pq_mls_message.extract()) {
                    (MlsMessageBodyIn::PublicMessage(t), MlsMessageBodyIn::PublicMessage(pq)) => {
                        Some(ApqProposalIn::new(t, pq))
                    }
                    _ => None,
                }
            })
            .collect();

        let t_ciphersuite = t_group_info.ciphersuite();
        let group_info_signer = group_info_signer_credential(&t_group_info, &t_ratchet_tree);
        let group_info = VerifiableApqGroupInfo::new(t_group_info, pq_group_info);

        let encrypted_profile_keys_fallback = Self::encrypted_profile_keys_fallback(
            &t_ratchet_tree,
            encrypted_user_profile_keys,
            &indexed_encrypted_user_profile_keys,
            own_user_id,
        );

        // Every leaf of the self group contains the QS client reference of its
        // device. We need to make sure the reference is preserved during a
        // resync. The DS rejects a resync that changes the client references.
        let replaced_client_reference = match signer {
            LeafSigningKey::User(_) => None,
            LeafSigningKey::SelfGroup(_) => own_client_reference_extension(&t_ratchet_tree, signer),
        };

        let ratchet_tree = ApqRatchetTreeIn::new(t_ratchet_tree, pq_ratchet_tree);

        // Build the group
        let mls_group_config = default_mls_group_join_config();
        let mut leaf_node_extensions =
            ClientAppData::current_for(vc_group_id.is_some()).leaf_node_extensions();
        if let Some(extension) = replaced_client_reference {
            leaf_node_extensions.add(extension)?;
        }
        let capabilities = match signer {
            LeafSigningKey::User(_) => default_leaf_node_capabilities(),
            LeafSigningKey::SelfGroup(_) => self_group_leaf_node_capabilities(),
        };
        let leaf_node_params = LeafNodeParameters::builder()
            .with_capabilities(capabilities)
            .with_extensions(leaf_node_extensions)
            .build();

        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let mut builder = ApqExternalCommitBuilder::new()
            .with_ratchet_tree(ratchet_tree)
            .with_proposals(proposals)
            .with_aad(aad.tls_serialize_detached()?)
            .with_config(mls_group_config)
            .skip_lifetime_validation()
            .leaf_node_parameters(leaf_node_params.clone(), leaf_node_params)
            .create_group_info(true)
            // The self group is the emulation group, so rejoining it has to
            // re-register the derivation epoch the external commit creates.
            .emulation_group(matches!(signer, LeafSigningKey::SelfGroup(_)));
        if let Some(group_id) = vc_group_id {
            builder = builder.vc_emulation(group_id);
        }
        if let Some(hash) = connection_group
            .as_ref()
            .and_then(|join| join.connection_offer_hash)
        {
            let psk_id = store_connection_offer_psk(&provider, t_ciphersuite, hash)?;
            builder = builder.add_t_psk_proposal(PreSharedKeyProposal::new(psk_id));
        }
        // As in the welcome path, the PQ leaf carries an empty credential.
        let res = builder.build(
            &provider,
            signer,
            credential_with_key,
            group_info,
            |_, _| true,
        );
        let (apq_mls_group, commit_bundle) = match res {
            Ok(built) => built,
            Err(ApqExternalCommitBuilderError::BuildCommit(error)) => {
                return Ok(Err(to_capabilities_mismatch(error)?));
            }
            Err(error) => return Err(error.into()),
        };
        let (t_group, pq_group) = apq_mls_group.into_groups();

        // As in the welcome path, the signing key is what binds a member's two
        // leaves together. The builder has already run the structural checks.
        verify_pq_signature_keys(&t_group, &pq_group)
            .context("T and PQ membership is not bound by matching signature keys")?;

        let is_self_group = OwnClientInfo::is_own_self_group(&mut *txn, t_group.group_id()).await?;
        ensure_self_group_flag(&t_group, Some(&pq_group), is_self_group)?;
        // The self group must be rejoined with the per-device self-group key, other groups with
        // the user key. A mismatch means the caller resolved the signer for the wrong group.
        ensure!(
            is_self_group == matches!(signer, LeafSigningKey::SelfGroup(_)),
            "signer does not match the group's self-group status"
        );

        // Verify credentials (T only). Self-group leaves carry a self-group credential, which is
        // only accepted inside our own self group.
        let credentials =
            verify_member_credentials(&mut *txn, api_clients, &t_group, is_self_group).await?;
        if let Some(tombstone) = GroupAppData::tombstone(t_group.extensions()) {
            let closer = closer(group_info_signer.as_ref(), own_user_id)?;
            bail!(GroupClosed { closer, tombstone });
        }
        if let Some(join) = connection_group {
            add_joiner_to_room_state(&mut room_state, join.inviter, own_user_id)?;
        }
        ensure_room_state_matches_members(&room_state, &t_group)?;

        // Store the group, credentials and member profile infos
        let group = Self::new(
            identity_link_wrapper_key,
            group_state_ear_key,
            t_group,
            Some(pq_group),
            room_state,
            own_user_id.clone(),
        );

        Group::delete_from_db(txn, group.group_id()).await?;
        let member_profile_info = group
            .store_after_join(
                txn,
                credentials,
                indexed_encrypted_user_profile_keys,
                encrypted_profile_keys_fallback,
            )
            .await?;

        Ok(Ok((group, commit_bundle, member_profile_info)))
    }
}

fn own_client_reference_extension(
    ratchet_tree: &RatchetTreeIn,
    signer: &LeafSigningKey,
) -> Option<Extension> {
    let signature_key = SignaturePublicKey::from(signer.verifying_key().clone());
    let leaf = ratchet_tree
        .leaves()
        .find(|leaf| leaf.signature_key() == &signature_key)?
        .clone()
        .into_unchecked();
    let client_reference = leaf
        .extensions()
        .unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE)?
        .clone();
    Some(Extension::Unknown(
        QS_CLIENT_REFERENCE_EXTENSION_TYPE,
        client_reference,
    ))
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::keys::{LeafSigningKey, SelfGroupSigningKey},
        crypto::aead::keys::IdentityLinkWrapperKey,
        identifiers::{QsReference, QualifiedGroupId, UserId},
    };
    use airprotos::client::group::GroupData;
    use openmls::{prelude::GroupId, treesync::RatchetTreeIn};
    use uuid::Uuid;

    use super::own_client_reference_extension;
    use crate::{
        db::access::{DbAccess, WriteConnection},
        groups::{Group, NewGroupContext, client_reference_extension},
        utils::{persistence::open_db_in_memory, random_client_reference},
    };

    fn random_group_id() -> GroupId {
        GroupId::from(QualifiedGroupId::new(
            Uuid::new_v4(),
            "example.com".parse().unwrap(),
        ))
    }

    fn self_group_signer() -> anyhow::Result<LeafSigningKey> {
        Ok(LeafSigningKey::SelfGroup(SelfGroupSigningKey::generate(
            Uuid::new_v4(),
        )?))
    }

    /// The T ratchet tree of a new self group created by `signer`.
    async fn self_group_ratchet_tree(
        signer: &LeafSigningKey,
        creator_client_reference: Option<&QsReference>,
    ) -> anyhow::Result<RatchetTreeIn> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        connection
            .with_transaction(async |txn| -> anyhow::Result<_> {
                let (group, _params) = Group::create_apq_group(
                    &mut *txn,
                    signer,
                    UserId::random("example.com".parse()?),
                    IdentityLinkWrapperKey::random()?,
                    random_group_id(),
                    random_group_id(),
                    NewGroupContext::SelfGroup(GroupData::empty()),
                    None,
                    creator_client_reference,
                )?;
                Ok(group.mls_group().export_ratchet_tree().into())
            })
            .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn own_client_reference_is_read_from_own_leaf() -> anyhow::Result<()> {
        let signer = self_group_signer()?;
        let client_reference = random_client_reference();
        let ratchet_tree = self_group_ratchet_tree(&signer, Some(&client_reference)).await?;
        assert_eq!(
            own_client_reference_extension(&ratchet_tree, &signer),
            Some(client_reference_extension(&client_reference)?)
        );
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn own_leaf_without_client_reference() -> anyhow::Result<()> {
        let signer = self_group_signer()?;
        let ratchet_tree = self_group_ratchet_tree(&signer, None).await?;
        assert_eq!(own_client_reference_extension(&ratchet_tree, &signer), None);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn client_reference_of_another_leaf_is_ignored() -> anyhow::Result<()> {
        let creator = self_group_signer()?;
        let ratchet_tree =
            self_group_ratchet_tree(&creator, Some(&random_client_reference())).await?;
        assert_eq!(
            own_client_reference_extension(&ratchet_tree, &self_group_signer()?),
            None
        );
        Ok(())
    }
}
