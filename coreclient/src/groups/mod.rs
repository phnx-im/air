// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod client_auth_info;
// TODO: Allowing dead code here for now. We'll need diffs when we start
// rotating keys.
pub(crate) mod debug_info;
#[allow(dead_code)]
pub(crate) mod diff;
pub(crate) mod error;
pub(crate) mod openmls_provider;
pub(crate) mod persistence;
pub(crate) mod process;

pub(crate) use error::*;
pub(crate) use persistence::VerifiedGroup;

use aircommon::{
    credentials::{
        ClientCredential, GroupStorageWitness, VerifiableClientCredential, keys::ClientSigningKey,
    },
    crypto::{
        aead::{
            AeadDecryptable, AeadEncryptable,
            keys::{
                EncryptedUserProfileKey, GroupStateEarKey, IdentityLinkWrapperKey,
                WelcomeAttributionInfoEarKey,
            },
        },
        hpke::{HpkeDecryptable, JoinerInfoDecryptionKey},
        indexed_aead::keys::UserProfileKey,
        signatures::signable::{Signable, Verifiable},
    },
    identifiers::{QsReference, QualifiedGroupId, UserId},
    messages::{
        client_as::ConnectionOfferHash,
        client_ds::{
            AadMessage, AadPayload, DsJoinerInformation, GroupOperationParamsAad, WelcomeBundle,
        },
        client_ds_out::{
            AddUsersInfoOut, CreateGroupParamsOut, DeleteGroupParamsOut, ExternalCommitInfoIn,
            GroupOperationParamsOut, SelfRemoveParamsOut, SendMessageParamsOut,
            TargetedMessageParamsOut, TargetedMessageType, WelcomeInfoIn,
        },
        welcome_attribution_info::{
            WelcomeAttributionInfo, WelcomeAttributionInfoPayload, WelcomeAttributionInfoTbs,
        },
    },
    mls_group_config::{
        AIR_COMPONENT_ID, GROUP_DATA_EXTENSION_TYPE, MAX_PAST_EPOCHS, SUPPORTED_COMPONENTS,
        default_app_data_dictionary_extension, default_group_required_extensions,
        default_leaf_node_capabilities, default_leaf_node_extensions,
        default_mls_group_join_config, default_required_group_capabilities,
        default_sender_ratchet_configuration,
    },
    time::TimeStamp,
    utils::removed_client,
};
use airprotos::client::component::AirComponent;
use anyhow::{Context, Result, anyhow, bail, ensure};
use mimi_content::MimiContent;
use mimi_room_policy::{MimiProposal, RoleIndex, RoomPolicy, VerifiedRoomState};
use mls_assist::{components::ComponentsList, messages::AssistedMessageOut};
use openmls_provider::AirOpenMlsProvider;
use openmls_traits::storage::StorageProvider;
use serde::Serialize;
use tls_codec::DeserializeBytes;
use tracing::{Level, debug, enabled, error};

use crate::{
    SystemMessage,
    chats::messages::TimestampedMessage,
    clients::{
        api_clients::ApiClients,
        block_contact::{BlockedContact, BlockedContactError},
        targeted_message::TargetedMessageContent,
    },
    contacts::ContactAddInfos,
    db_access::{WriteConnection, WriteDbTransaction},
    groups::client_auth_info::VerifiableClientCredentialExt,
    key_stores::as_credentials::AsCredentials,
    outbound_service::resync::Resync,
};
use std::collections::HashSet;

use openmls::{
    component::ComponentType,
    group::{
        CreateCommitError, ExternalCommitBuilder, JoinBuilder, ProcessedWelcome,
        ProposalValidationError,
    },
    key_packages::KeyPackageBundle,
    prelude::{
        AppDataDictionaryExtension, BasicCredentialError, CredentialWithKey, Extension, Extensions,
        GroupId, KeyPackage, LeafNode, LeafNodeIndex, LeafNodeParameters, MlsGroup,
        MlsMessageBodyIn, MlsMessageIn, MlsMessageOut, OpenMlsProvider,
        PURE_PLAINTEXT_WIRE_FORMAT_POLICY, PreSharedKeyProposal, Proposal, ProposalType,
        ProtocolVersion, QueuedProposal, Sender, SignaturePublicKey, StagedCommit,
        UnknownExtension, tls_codec::Serialize as TlsSerializeTrait,
    },
    schedule::{ExternalPsk, PreSharedKeyId, Psk},
    treesync::{RatchetTree, errors::LeafNodeValidationError},
};

use self::{client_auth_info::StorableClientCredential, diff::StagedGroupDiff};

pub(crate) struct PartialCreateGroupParams {
    pub(crate) group_id: GroupId,
    ratchet_tree: RatchetTree,
    group_info: MlsMessageOut,
    pub(crate) room_state: VerifiedRoomState,
}

impl PartialCreateGroupParams {
    pub(crate) fn into_params(
        self,
        creator_client_reference: QsReference,
        encrypted_user_profile_key: EncryptedUserProfileKey,
    ) -> CreateGroupParamsOut {
        CreateGroupParamsOut {
            group_id: self.group_id,
            ratchet_tree: self.ratchet_tree,
            encrypted_user_profile_key,
            creator_client_reference,
            group_info: self.group_info,
            room_state: self.room_state,
        }
    }
}

#[derive(Debug)]
pub(super) struct ProfileInfo {
    pub(super) client_credential: ClientCredential,
    pub(super) user_profile_key: UserProfileKey,
}

impl From<(ClientCredential, UserProfileKey)> for ProfileInfo {
    fn from((client_credential, user_profile_key): (ClientCredential, UserProfileKey)) -> Self {
        Self {
            client_credential,
            user_profile_key,
        }
    }
}

/// Bytes stored in the group data extension.
#[derive(Debug, PartialEq, Clone)]
pub(crate) struct GroupDataBytes {
    bytes: Vec<u8>,
}

impl GroupDataBytes {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn from_staged_commit(staged_commit: &StagedCommit) -> Option<Self> {
        staged_commit.queued_proposals().find_map(|p| {
            if let Proposal::GroupContextExtensions(extensions) = p.proposal()
                && let Some(ext) = extensions.extensions().unknown(GROUP_DATA_EXTENSION_TYPE)
            {
                Some(GroupDataBytes::from(ext.0.clone()))
            } else {
                None
            }
        })
    }
}

impl From<Vec<u8>> for GroupDataBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

#[derive(Debug)]
pub(crate) struct Group {
    group_id: GroupId,
    identity_link_wrapper_key: IdentityLinkWrapperKey,
    group_state_ear_key: GroupStateEarKey,
    mls_group: MlsGroup,
    pub room_state: VerifiedRoomState,
    pending_diff: Option<StagedGroupDiff>, // Currently unused, but we're keeping it for later
    /// The time at which the user self-updated their key material in this group the last time
    pub(crate) self_updated_at: Option<TimeStamp>,
}

impl Group {
    pub(crate) fn mls_group(&self) -> &MlsGroup {
        &self.mls_group
    }

    /// Create a group.
    pub(super) fn create_group(
        mut connection: impl WriteConnection,
        signer: &ClientSigningKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        group_id: GroupId,
        group_data_bytes: GroupDataBytes,
    ) -> Result<(Self, PartialCreateGroupParams)> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let group_state_ear_key = GroupStateEarKey::random()?;

        let required_capabilities =
            Extension::RequiredCapabilities(default_group_required_extensions());

        let group_data_extension = Extension::Unknown(
            GROUP_DATA_EXTENSION_TYPE,
            UnknownExtension(group_data_bytes.bytes),
        );
        let gc_extensions =
            Extensions::from_vec(vec![group_data_extension, required_capabilities])?;

        let credential_with_key = CredentialWithKey {
            credential: signer.credential().try_into()?,
            signature_key: signer.credential().verifying_key().clone().into(),
        };

        let mls_group = MlsGroup::builder()
            .with_group_id(group_id.clone())
            .with_capabilities(default_required_group_capabilities())
            .with_group_context_extensions(gc_extensions)
            .sender_ratchet_configuration(default_sender_ratchet_configuration())
            .max_past_epochs(MAX_PAST_EPOCHS)
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .build(&provider, signer, credential_with_key)
            .map_err(|e| anyhow!("Error while creating group: {:?}", e))?;

        let user_id = signer.credential().user_id();
        let room_state = VerifiedRoomState::new(
            user_id.tls_serialize_detached()?,
            RoomPolicy::default_trusted_private(),
        )
        .unwrap();

        let params = PartialCreateGroupParams {
            group_id: group_id.clone(),
            ratchet_tree: mls_group.export_ratchet_tree(),
            group_info: mls_group.export_group_info(provider.crypto(), signer, true)?,
            room_state: room_state.clone(),
        };

        let group = Self {
            group_id,
            identity_link_wrapper_key,
            mls_group,
            room_state,
            group_state_ear_key: group_state_ear_key.clone(),
            pending_diff: None,
            self_updated_at: Some(TimeStamp::now()),
        };

        Ok((group, params))
    }

    /// Join a group with the provided welcome message. If there exists a group
    /// with the same ID, checks if that group is inactive and if so deletes the
    /// old group before it stores the new one.
    ///
    /// Returns the group name, sender user id and the list of profile keys of the members.
    pub(super) async fn join_group(
        welcome_bundle: WelcomeBundle,
        // This is our own key that the sender uses to encrypt to us. We should
        // be able to retrieve it from the client's key store.
        welcome_attribution_info_ear_key: &WelcomeAttributionInfoEarKey,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
    ) -> Result<(Self, UserId, Vec<ProfileInfo>)> {
        let serialized_welcome = welcome_bundle.welcome.tls_serialize_detached()?;

        let mls_group_config = default_mls_group_join_config();

        let (processed_welcome, joiner_info) = {
            // Phase 1: Fetch the right KeyPackageBundle from storage
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            let kpb: KeyPackageBundle = welcome_bundle
                .welcome
                .welcome
                .secrets()
                .iter()
                .find_map(|egs| {
                    let kp_hash = egs.new_member();
                    match provider.storage().key_package(&kp_hash) {
                        Ok(Some(kpb)) => Some(kpb),
                        _ => None,
                    }
                })
                .ok_or(GroupOperationError::MissingKeyPackage)?;

            // Phase 2: Process the welcome message
            let private_key = kpb.init_private_key();
            let info = &[];
            let aad = &[];
            let decryption_key = JoinerInfoDecryptionKey::from((
                private_key.clone(),
                kpb.key_package().hpke_init_key().clone(),
            ));
            let joiner_info = DsJoinerInformation::decrypt(
                welcome_bundle.encrypted_joiner_info,
                &decryption_key,
                info,
                aad,
            )?;

            let processed_welcome = ProcessedWelcome::new_from_welcome(
                &provider,
                &mls_group_config,
                welcome_bundle.welcome.welcome,
            )?;

            // Phase 3: Check if there is already a group with the same ID.
            let group_id = processed_welcome.unverified_group_info().group_id().clone();
            if let Some(group) = Self::load(&mut *txn, &group_id).await? {
                // If the group is active, we can't join it.
                if group.mls_group().is_active() {
                    bail!("We can't join a group that is still active.");
                }
                // Otherwise, we delete the old group.
                Self::delete_from_db(txn, &group_id).await?;
            }
            (processed_welcome, joiner_info)
        };

        // Phase 4: Fetch the welcome info from the server
        let group_id = processed_welcome.unverified_group_info().group_id();
        let epoch = processed_welcome.unverified_group_info().epoch();
        let qgid = QualifiedGroupId::try_from(group_id)?;
        let welcome_info = api_clients
            .get(qgid.owning_domain())?
            .ds_welcome_info(
                group_id.clone(),
                epoch,
                &joiner_info.group_state_ear_key,
                signer,
            )
            .await?;

        let WelcomeInfoIn {
            ratchet_tree,
            encrypted_user_profile_keys,
            room_state,
        } = welcome_info;

        let (mls_group, joiner_info, welcome_attribution_info, sender_user_id) = {
            // Phase 5: Finish processing the welcome message
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            let staged_welcome = JoinBuilder::new(&provider, processed_welcome)
                // We skip lifetime validation for now.
                .skip_lifetime_validation()
                .with_ratchet_tree(ratchet_tree)
                .build()?;

            let mls_group = staged_welcome.into_group(&provider)?;

            // Decrypt WelcomeAttributionInfo
            let verifiable_attribution_info = WelcomeAttributionInfo::decrypt(
                welcome_attribution_info_ear_key,
                &welcome_bundle.encrypted_attribution_info,
            )?
            .into_verifiable(mls_group.group_id().clone(), serialized_welcome);

            let sender_user_id = verifiable_attribution_info.sender();
            let sender_client_credential =
                StorableClientCredential::load_by_user_id(&mut *txn, &sender_user_id)
                    .await?
                    .ok_or_else(|| {
                        anyhow!("Could not find client credential of sender in database.")
                    })?;

            if BlockedContact::check_blocked(&mut *txn, &sender_user_id).await? {
                bail!(BlockedContactError);
            }

            let welcome_attribution_info: WelcomeAttributionInfoPayload =
                verifiable_attribution_info.verify(sender_client_credential.verifying_key())?;

            (
                mls_group,
                joiner_info,
                welcome_attribution_info,
                sender_user_id,
            )
        };

        let credentials = verify_member_credentials(&mut *txn, api_clients, &mls_group).await?;

        let group = Self {
            group_id: mls_group.group_id().clone(),
            mls_group,
            identity_link_wrapper_key: welcome_attribution_info.identity_link_wrapper_key().clone(),
            group_state_ear_key: joiner_info.group_state_ear_key,
            pending_diff: None,
            room_state,
            self_updated_at: Some(TimeStamp::now()),
        };

        // Phase 7: Store the group and client credentials.
        group.store(&mut *txn).await?;
        for credential in &credentials {
            credential.store(&mut *txn).await?;
        }

        // Phase 8: Decrypt profile keys
        let member_profile_info = encrypted_user_profile_keys
            .into_iter()
            .zip(credentials)
            .map(|(eupk, ci)| {
                UserProfileKey::decrypt(
                    welcome_attribution_info.identity_link_wrapper_key(),
                    &eupk,
                    ci.user_id(),
                )
                .map(|user_profile_key| ProfileInfo {
                    user_profile_key,
                    client_credential: ci.into(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok((group, sender_user_id, member_profile_info))
    }

    /// Join a group using an external commit.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn join_group_externally(
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        external_commit_info: ExternalCommitInfoIn,
        signer: &ClientSigningKey,
        group_state_ear_key: GroupStateEarKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        aad: AadMessage,
        // Should be Some if this join is in response to a connection offer.
        connection_offer_hash: Option<ConnectionOfferHash>,
    ) -> anyhow::Result<
        Result<(Self, MlsMessageOut, MlsMessageOut, Vec<ProfileInfo>), LeafNodeValidationError>,
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
            room_state,
            proposals,
        } = external_commit_info;

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

        // Figure out who was removed so we can filter out the encrypted profile keys later.
        let removed_members: Vec<_> = proposals
            .iter()
            .filter_map(|pm| {
                let Sender::Member(sender) = pm.sender() else {
                    return None;
                };
                Some(*sender)
            })
            .collect();

        // Let's create the group first so that we can access the GroupId.
        // Phase 1: Create and store the group
        let (mls_group, commit, group_info) = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            // Prepare PSK proposal if we have a connection offer hash.
            let psk_proposal = match connection_offer_hash {
                Some(co_hash) => {
                    let psk_value = co_hash.into_bytes();
                    let psk_id = PreSharedKeyId::new(
                        verifiable_group_info.ciphersuite(),
                        provider.rand(),
                        Psk::External(ExternalPsk::new(psk_value.to_vec())),
                    )?;
                    psk_id.store(&provider, &psk_value)?;
                    Some(PreSharedKeyProposal::new(psk_id))
                }
                None => None,
            };

            let leaf_node_parameters = LeafNodeParameters::builder()
                .with_capabilities(default_leaf_node_capabilities())
                .with_extensions(default_leaf_node_extensions())
                .build();

            let mut builder = ExternalCommitBuilder::new()
                .with_proposals(proposals)
                .with_aad(aad.tls_serialize_detached()?)
                .with_config(mls_group_config)
                .skip_lifetime_validation()
                .with_ratchet_tree(ratchet_tree_in)
                .build_group(&provider, verifiable_group_info, credential_with_key)?
                .leaf_node_parameters(leaf_node_parameters);

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
            )
        };

        // Phase 3: Verify the client credentials
        let credentials = verify_member_credentials(&mut *txn, api_clients, &mls_group).await?;

        let group = Self {
            group_id: mls_group.group_id().clone(),
            mls_group,
            identity_link_wrapper_key,
            group_state_ear_key,
            pending_diff: None,
            room_state,
            self_updated_at: Some(TimeStamp::now()),
        };

        // Phase 4: Store the group and client auth info.
        // If the group previously existed, delete it first.
        Group::delete_from_db(txn, &group.group_id).await?;
        group.store(&mut *txn).await?;
        for credential in &credentials {
            credential.store(&mut *txn).await?;
        }
        // Also store own credential
        let own_credential = signer.credential().clone();
        StorableClientCredential::from(own_credential)
            .store(&mut *txn)
            .await?;

        // Compile a list of user profile keys for the members.
        let member_profile_info = encrypted_user_profile_keys
            .into_iter()
            .enumerate()
            .filter_map(|(index, eupk)| {
                (!removed_members.contains(&LeafNodeIndex::new(index as u32))).then_some(eupk)
            })
            .zip(credentials)
            .map(|(eupk, ci)| {
                UserProfileKey::decrypt(&group.identity_link_wrapper_key, &eupk, ci.user_id()).map(
                    |user_profile_key| ProfileInfo {
                        user_profile_key,
                        client_credential: ci.into(),
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Ok((group, commit, group_info.into(), member_profile_info)))
    }

    /// Invite the given list of contacts to join the group.
    ///
    /// Returns the [`AddUserParamsOut`] as input for the API client.
    pub(super) async fn stage_invite(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &ClientSigningKey,
        // The following three vectors have to be in sync, i.e. of the same length
        // and refer to the same contacts in order.
        add_infos: Vec<ContactAddInfos>,
        wai_keys: Vec<WelcomeAttributionInfoEarKey>,
        client_credentials: Vec<ClientCredential>,
    ) -> anyhow::Result<Result<GroupOperationParamsOut, LeafNodeValidationError>> {
        debug_assert!(add_infos.len() == wai_keys.len());
        debug_assert!(add_infos.len() == client_credentials.len());
        // Prepare KeyPackages

        let (key_packages, user_profile_keys): (Vec<KeyPackage>, Vec<UserProfileKey>) = add_infos
            .into_iter()
            .map(|ai| (ai.key_package, ai.user_profile_key))
            .unzip();

        let new_encrypted_user_profile_keys = user_profile_keys
            .iter()
            .zip(client_credentials.iter())
            .map(|(upk, client_credential)| {
                upk.encrypt(&self.identity_link_wrapper_key, client_credential.user_id())
            })
            .collect::<Result<Vec<_>, _>>()?;

        let aad_message: AadMessage = AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys,
        })
        .into();

        // Set Aad to contain the encrypted client credentials.
        let (mls_commit, welcome_option, group_info_option) = {
            let provider = AirOpenMlsProvider::new(connection.as_mut());
            self.mls_group
                .set_aad(aad_message.tls_serialize_detached()?);
            let res = self
                .mls_group
                .commit_builder()
                .force_self_update(true)
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

    pub(super) async fn stage_remove(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &ClientSigningKey,
        mut members: Vec<UserId>,
    ) -> Result<GroupOperationParamsOut> {
        // Note: The order of `remove_indices` is not the same as the order of `members`.
        let mut remove_indices = Vec::with_capacity(members.len());
        for member in self.mls_group.members() {
            let credential = VerifiableClientCredential::from_basic_credential(&member.credential)?;
            let user_id = credential.user_id();
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
        let provider = AirOpenMlsProvider::new(connection.as_mut());

        let (mls_message, _welcome_option, group_info_option) = self
            .mls_group
            .commit_builder()
            .force_self_update(true)
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

    pub(super) async fn stage_delete(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &ClientSigningKey,
    ) -> anyhow::Result<DeleteGroupParamsOut> {
        let provider = &AirOpenMlsProvider::new(connection.as_mut());
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

        // There shouldn't be a welcome
        let aad_payload = AadPayload::DeleteGroup;
        let aad = AadMessage::from(aad_payload).tls_serialize_detached()?;
        self.mls_group.set_aad(aad);

        let (mls_message, _welcome_option, group_info_option) = self
            .mls_group
            .commit_builder()
            .force_self_update(true)
            .propose_removals(remove_indices)
            .load_psks(provider.storage())?
            .create_group_info(true)
            .build(provider.rand(), provider.crypto(), signer, |_| true)?
            .stage_commit(provider)?
            .into_contents();

        debug_assert!(_welcome_option.is_none());
        let group_info =
            group_info_option.ok_or(anyhow!("No group info after commit operation"))?;
        let commit = AssistedMessageOut::new(mls_message, Some(group_info.into()));

        let params = DeleteGroupParamsOut { commit };
        Ok(params)
    }

    pub(super) async fn discard_pending_commit(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
    ) -> Result<()> {
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        self.pending_diff = None;
        self.mls_group.clear_pending_commit(provider.storage())?;
        Ok(())
    }

    /// Applies the staged operations of the given `StagedCommit` to the room
    /// state of this group. If no `StagedCommit` is given, apply the operation
    /// of the pending commit of this group, if any.
    fn apply_staged_operations_to_room_state(
        &mut self,
        staged_commit: Option<&'_ StagedCommit>,
    ) -> Result<()> {
        for (remover, removed) in self.staged_commit_removes(staged_commit) {
            self.room_state_change_role(&remover, &removed, RoleIndex::Outsider)?;
        }
        for (adder, added) in self.pending_adds(staged_commit) {
            self.room_state_change_role(&adder, &added, RoleIndex::Regular)?;
        }

        Ok(())
    }

    /// If a [`StagedCommit`] is given, merge it and apply the pending group
    /// diff. If no [`StagedCommit`] is given, merge any pending commit and
    /// apply the pending group diff.
    ///
    /// Returns the messages resulting from the commit and any group data
    /// extracted from the staged commit.
    pub(in crate::groups) async fn merge_pending_commit(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        verified: &impl GroupStorageWitness,
        staged_commit_option: impl Into<Option<StagedCommit>>,
        ds_timestamp: TimeStamp,
    ) -> Result<(Vec<TimestampedMessage>, Option<GroupDataBytes>)> {
        let staged_commit_option: Option<StagedCommit> = staged_commit_option.into();

        self.apply_staged_operations_to_room_state(staged_commit_option.as_ref())?;

        let (event_messages, group_data) = if let Some(staged_commit) = staged_commit_option {
            // Compute the messages we want to emit from the staged commit and the
            // client info diff.
            let staged_commit_messages = TimestampedMessage::from_staged_commit(
                self,
                verified,
                &staged_commit,
                ds_timestamp,
            )?;

            let group_data = GroupDataBytes::from_staged_commit(&staged_commit);

            let provider = AirOpenMlsProvider::new(txn.as_mut());
            self.mls_group
                .merge_staged_commit(&provider, staged_commit)?;
            (staged_commit_messages, group_data)
        } else {
            // If we're merging a pending commit, we need to check if we have
            // committed a remove proposal by reference. If we have, we need to
            // create a notification message.
            let (staged_commit_messages, group_data) =
                if let Some(staged_commit) = self.mls_group.pending_commit() {
                    let group_data = GroupDataBytes::from_staged_commit(staged_commit);
                    let messages = TimestampedMessage::from_staged_commit(
                        self,
                        verified,
                        staged_commit,
                        ds_timestamp,
                    )?;
                    (messages, group_data)
                } else {
                    (vec![], None)
                };
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            self.mls_group.merge_pending_commit(&provider)?;
            (staged_commit_messages, group_data)
        };

        // We now apply the diff (if present)
        if let Some(diff) = self.pending_diff.take() {
            if let Some(identity_link_wrapper_key) = diff.identity_link_wrapper_key {
                self.identity_link_wrapper_key = identity_link_wrapper_key;
            }
            if let Some(group_state_ear_key) = diff.group_state_ear_key {
                self.group_state_ear_key = group_state_ear_key;
            }
        }

        self.pending_diff = None;
        Ok((event_messages, group_data))
    }

    /// Send an application message to the group.
    pub(super) fn create_message(
        &mut self,
        provider: &impl OpenMlsProvider,
        signer: &ClientSigningKey,
        content: MimiContent,
    ) -> Result<SendMessageParamsOut, GroupOperationError> {
        let mls_message = self
            .mls_group
            .create_message(provider, signer, &content.serialize()?)?;

        let message = AssistedMessageOut::new(mls_message, None);

        let suppress_notifications = suppress_notifications(&content);

        let send_message_params = SendMessageParamsOut {
            sender: self.mls_group.own_leaf_index(),
            message,
            suppress_notifications,
        };

        Ok(send_message_params)
    }

    /// Send an application message to the group.
    pub(super) fn create_targeted_application_message(
        &mut self,
        provider: &impl OpenMlsProvider,
        signer: &ClientSigningKey,
        recipient: UserId,
        content: TargetedMessageContent,
    ) -> Result<TargetedMessageParamsOut, GroupOperationError> {
        let content_bytes = content.tls_serialize_detached()?;
        let mls_message = self
            .mls_group
            .create_message(provider, signer, &content_bytes)?;

        let message = AssistedMessageOut::new(mls_message, None);

        let recipient_index = self
            .mls_group()
            .members()
            .find_map(|m| {
                let client_credential =
                    VerifiableClientCredential::from_basic_credential(&m.credential).ok()?;
                if client_credential.user_id() == &recipient {
                    Some(m.index)
                } else {
                    None
                }
            })
            .ok_or(TargetedMessageError::RecipientNotInGroup)?;

        let params = TargetedMessageParamsOut {
            sender: self.mls_group.own_leaf_index(),
            message_type: TargetedMessageType::ApplicationMessage {
                message,
                recipient: recipient_index,
            },
        };

        Ok(params)
    }

    /// Get a reference to the group's group id.
    pub(crate) fn group_id(&self) -> &GroupId {
        self.mls_group().group_id()
    }

    pub(crate) fn group_state_ear_key(&self) -> &GroupStateEarKey {
        &self.group_state_ear_key
    }

    pub(crate) fn identity_link_wrapper_key(&self) -> &IdentityLinkWrapperKey {
        &self.identity_link_wrapper_key
    }

    /// Returns an iterator over [`UserId`]s of the members of the group.
    pub(crate) fn members(&self) -> impl Iterator<Item = UserId> {
        self.mls_group.members().filter_map(|m| {
            let credential = VerifiableClientCredential::from_basic_credential(&m.credential)
                .inspect_err(|error| {
                    error!(%error, "Invalid member credential");
                })
                .ok()?;
            Some(credential.user_id().clone())
        })
    }

    pub(super) async fn update(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        new_group_data: Option<GroupDataBytes>,
    ) -> Result<GroupOperationParamsOut> {
        // We don't expect there to be a welcome.
        let aad = AadMessage::from(AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys: Vec::new(),
        }))
        .tls_serialize_detached()?;

        let extensions = new_group_data
            .map(|gd| -> Result<_> {
                let group_data_extension =
                    Extension::Unknown(GROUP_DATA_EXTENSION_TYPE, UnknownExtension(gd.bytes));
                let mut exts = self.mls_group().extensions().clone();
                exts.add_or_replace(group_data_extension)?;
                Ok(exts)
            })
            .transpose()?;

        let own_leaf_node = self.mls_group.own_leaf_node().context("No own leaf node")?;
        let leaf_node_parameters = Self::update_leaf_node_extensions(own_leaf_node.extensions())?;

        self.mls_group.set_aad(aad);
        let (mls_message, group_info) = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());

            let mut builder = self.mls_group.commit_builder();
            if let Some(extensions) = extensions {
                builder = builder.propose_group_context_extensions(extensions)?;
            };

            let (mls_message, _welcome_option, group_info_option) = builder
                .force_self_update(true)
                .leaf_node_parameters(leaf_node_parameters)
                .load_psks(provider.storage())?
                .create_group_info(true)
                .build(provider.rand(), provider.crypto(), signer, |_| true)?
                .stage_commit(&provider)?
                .into_contents();

            (
                mls_message,
                group_info_option.ok_or_else(|| anyhow!("No group info after commit"))?,
            )
        };

        let commit = AssistedMessageOut::new(mls_message, Some(group_info.into()));
        Ok(GroupOperationParamsOut {
            commit,
            add_users_info_option: None,
        })
    }

    fn update_leaf_node_extensions(
        leaf_node_extensions: &Extensions<LeafNode>,
    ) -> anyhow::Result<LeafNodeParameters> {
        let mut leaf_node_parameters =
            LeafNodeParameters::builder().with_capabilities(default_leaf_node_capabilities());

        if let Some(app_data_dictionary) = leaf_node_extensions.app_data_dictionary() {
            let dict = app_data_dictionary.dictionary();
            let mut updated_dict = None;

            // Augment app components
            if let Some(mut app_components) = dict
                .get(&ComponentType::AppComponents.into())
                .and_then(|data| {
                    ComponentsList::tls_deserialize_exact_bytes(data)
                        .inspect_err(|error| {
                            error!(%error, "Failed to deserialize app components; will replace");
                        })
                        .ok()
                })
            {
                if !app_components.component_ids.contains(&AIR_COMPONENT_ID) {
                    // Advertise that we support the Air component in the app data dictionary.
                    app_components.component_ids.push(AIR_COMPONENT_ID);
                    updated_dict.get_or_insert_with(|| dict.clone()).insert(
                        ComponentType::AppComponents.into(),
                        app_components.tls_serialize_detached()?,
                    );
                }
            } else {
                // Add app components list to the app data dictionary.
                updated_dict.get_or_insert_with(|| dict.clone()).insert(
                    ComponentType::AppComponents.into(),
                    ComponentsList {
                        component_ids: SUPPORTED_COMPONENTS.to_vec(),
                    }
                    .tls_serialize_detached()?,
                );
            }

            // Augment Air component
            if let Some(mut air_component) = dict.get(&AIR_COMPONENT_ID).and_then(|data| {
                AirComponent::from_bytes(data)
                    .inspect_err(|error| {
                        error!(%error, "Failed to deserialize air component; will replace");
                    })
                    .ok()
            }) {
                // Enabled encrypted group profiles
                if !air_component.features.encrypted_group_profiles {
                    air_component.features.encrypted_group_profiles = true;
                    updated_dict
                        .get_or_insert_with(|| dict.clone())
                        .insert(AIR_COMPONENT_ID, air_component.to_bytes()?);
                }
            } else {
                // Add air component to the app data dictionary.
                updated_dict.get_or_insert_with(|| dict.clone()).insert(
                    AIR_COMPONENT_ID,
                    AirComponent::default_leaf_or_key_package_component()
                        .to_bytes()
                        .expect("invalid Air component"),
                );
            };

            if let Some(dict) = updated_dict {
                // Replace the app data dictionary with the updated one
                let mut leaf_node_extensions = leaf_node_extensions.clone();
                leaf_node_extensions.add_or_replace(Extension::AppDataDictionary(
                    AppDataDictionaryExtension::new(dict),
                ))?;
                leaf_node_parameters =
                    leaf_node_parameters.with_extensions(leaf_node_extensions.clone());
            }
        } else {
            // App data extension is not present, add it with default values
            let mut leaf_node_extensions = leaf_node_extensions.clone();
            leaf_node_extensions.add(default_app_data_dictionary_extension())?;
            leaf_node_parameters = leaf_node_parameters.with_extensions(leaf_node_extensions);
        }

        Ok(leaf_node_parameters.build())
    }

    pub(super) fn stage_leave_group(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &ClientSigningKey,
    ) -> Result<SelfRemoveParamsOut> {
        let provider = &AirOpenMlsProvider::new(connection.as_mut());
        let proposal = self
            .mls_group
            .leave_group_via_self_remove(provider, signer)?;

        let assisted_message = AssistedMessageOut::new(proposal, None);
        let params = SelfRemoveParamsOut {
            remove_proposal: assisted_message,
        };
        Ok(params)
    }

    pub(super) fn store_proposal(
        &mut self,
        mut connection: impl WriteConnection,
        proposal: QueuedProposal,
    ) -> Result<()> {
        let provider = &AirOpenMlsProvider::new(connection.as_mut());
        self.mls_group
            .store_pending_proposal(provider.storage(), proposal)?;
        Ok(())
    }

    /// Returns a list of (remover, removed) UserId pairs for pending remove proposals.
    pub(crate) fn pending_removes(&self) -> Vec<(UserId, UserId)> {
        self.compile_removed_list(self.mls_group().pending_proposals())
    }

    fn staged_commit_removes(
        &self,
        staged_commit: Option<&'_ StagedCommit>,
    ) -> Vec<(UserId, UserId)> {
        let Some(staged_commit) = staged_commit.or_else(|| self.mls_group().pending_commit())
        else {
            return Vec::new();
        };
        self.compile_removed_list(staged_commit.queued_proposals())
    }

    fn compile_removed_list<'a>(
        &self,
        removes: impl Iterator<Item = &'a QueuedProposal>,
    ) -> Vec<(UserId, UserId)> {
        let mut pending_removes = Vec::new();

        for proposal in removes {
            let Sender::Member(remover) = proposal.sender() else {
                // We don't support external senders yet.
                continue;
            };
            let remover = match self.user_id_at_index(*remover) {
                Some(user_id) => user_id,
                None => continue,
            };
            if let Some(removed_client_index) = removed_client(proposal)
                && let Some(removed) = self.user_id_at_index(removed_client_index)
            {
                pending_removes.push((remover, removed));
            }
        }
        pending_removes
    }

    /// Returns the `GroupData` of a pending GroupContextExtension change proposal, if any.
    #[expect(dead_code)]
    pub(crate) fn pending_group_data_update(&self) -> Option<GroupDataBytes> {
        let pending_commit = self.mls_group().pending_commit()?;
        GroupDataBytes::from_staged_commit(pending_commit)
    }

    fn user_id_at_index(&self, index: LeafNodeIndex) -> Option<UserId> {
        self.mls_group().member_at(index).and_then(|m| {
            VerifiableClientCredential::from_basic_credential(&m.credential)
                .map(|c| c.user_id().clone())
                .ok()
        })
    }

    /// Returns a list of (adder, added) UserId pairs for pending add proposals.
    pub(crate) fn pending_adds(
        &self,
        staged_commit: Option<&'_ StagedCommit>,
    ) -> Vec<(UserId, UserId)> {
        let staged_commit = staged_commit.or_else(|| self.mls_group().pending_commit());
        let mut pending_adds = Vec::new();
        let Some(pending_commit) = staged_commit else {
            return pending_adds;
        };
        for proposal in pending_commit.add_proposals() {
            let Sender::Member(adder_index) = proposal.sender() else {
                // We don't support external senders yet.
                continue;
            };
            let adder = match self.user_id_at_index(*adder_index) {
                Some(user_id) => user_id,
                None => continue,
            };
            let Ok(added_user) = VerifiableClientCredential::from_basic_credential(
                proposal
                    .add_proposal()
                    .key_package()
                    .leaf_node()
                    .credential(),
            )
            .map(|c| c.user_id().clone()) else {
                continue;
            };
            pending_adds.push((adder, added_user));
        }
        pending_adds
    }

    pub(crate) fn verify_role_change(
        &self,
        sender: &UserId,
        target: &UserId,
        role: RoleIndex,
    ) -> Result<()> {
        let sender = sender.tls_serialize_detached()?;
        let target = target.tls_serialize_detached()?;

        let result = self
            .room_state
            .can_apply_regular_proposals(&sender, &[MimiProposal::ChangeRole { target, role }]);

        Ok(result?)
    }

    pub(crate) fn room_state_change_role(
        &mut self,
        sender: &UserId,
        target: &UserId,
        role: RoleIndex,
    ) -> Result<()> {
        let sender = sender.tls_serialize_detached()?;
        let target = target.tls_serialize_detached()?;

        let result = self
            .room_state
            .apply_regular_proposals(&sender, &[MimiProposal::ChangeRole { target, role }]);

        Ok(result?)
    }

    pub(crate) fn group_data(&self) -> Option<GroupDataBytes> {
        self.mls_group().extensions().iter().find_map(|e| match e {
            Extension::Unknown(GROUP_DATA_EXTENSION_TYPE, extension_bytes) => {
                Some(GroupDataBytes::from(extension_bytes.0.clone()))
            }
            _ => None,
        })
    }

    pub(crate) fn own_index(&self) -> LeafNodeIndex {
        self.mls_group().own_leaf_index()
    }

    pub(crate) fn store_connection_offer_psk(
        &self,
        mut connection: impl WriteConnection,
        connection_offer_hash: ConnectionOfferHash,
    ) -> Result<()> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let psk_value = connection_offer_hash.into_bytes();
        PreSharedKeyId::new(
            self.mls_group().ciphersuite(),
            provider.rand(),
            Psk::External(ExternalPsk::new(
                connection_offer_hash.into_bytes().to_vec(),
            )),
        )?
        .store(&provider, &psk_value)?;
        Ok(())
    }

    /// Deserializes client credentials from the corresponding leaf node.
    ///
    /// Does not guarantee that the credential was verified and is valid.
    pub(crate) fn unverified_credential_at(
        &self,
        index: LeafNodeIndex,
    ) -> Result<Option<VerifiableClientCredential>, BasicCredentialError> {
        self.mls_group
            .member_at(index)
            .map(|m| VerifiableClientCredential::from_basic_credential(&m.credential))
            .transpose()
    }

    /// Same as [`Self::unverified_credential_at()`] but guarantees that the credential was
    /// verified and is valid (if leaf contains valid data).
    ///
    /// The guarantee is given by the presence of the `witness` argument.
    pub(crate) fn credential_at(
        &self,
        index: LeafNodeIndex,
        witness: &impl GroupStorageWitness,
    ) -> anyhow::Result<Option<ClientCredential>> {
        ensure!(self.group_id() == witness.group_id(), "Group ID mismatch");
        Ok(self
            .unverified_credential_at(index)?
            .map(|credential| ClientCredential::assume_verified(credential, witness)))
    }
}

/// Verify credentials of *all* members of the group.
///
/// Might do a network request to fetch AS credentials.
///
/// Returns the credentials of the group members.
async fn verify_member_credentials(
    txn: &mut WriteDbTransaction<'_>,
    api_clients: &ApiClients,
    mls_group: &MlsGroup,
) -> anyhow::Result<Vec<StorableClientCredential>> {
    let unverified_credentials = mls_group
        .members()
        .map(|m| {
            Ok((
                VerifiableClientCredential::from_basic_credential(&m.credential)?,
                SignaturePublicKey::from(m.signature_key),
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let as_credentials = AsCredentials::fetch_for_verification(
        txn,
        api_clients,
        unverified_credentials.iter().map(|(c, _)| c),
    )
    .await?;

    unverified_credentials
        .into_iter()
        .map(|(credential, leaf_verifying_key)| {
            VerifiableClientCredential::verify_and_validate(
                credential,
                &leaf_verifying_key,
                None,
                &as_credentials,
            )
        })
        .collect()
}

/// Cleans up local state when the DS reports that a group no longer exists.
///
/// Mirrors what happens when we process a deletion commit from another member:
/// the chat is marked inactive (preserving history) and the MLS group is
/// deleted. The group deletion cascades to `resync_queue`,
/// `pending_chat_operation`, and `group_membership` via foreign keys.
///
/// This function is idempotent — safe to call even if the group or chat is
/// already gone.
pub(crate) async fn handle_group_not_found_on_ds(
    txn: &mut WriteDbTransaction<'_>,
    group_id: &GroupId,
) -> anyhow::Result<()> {
    // Collect past members before deleting the group.
    let past_members = match Group::load(&mut *txn, group_id).await? {
        Some(group) => group.members().collect(),
        None => Vec::new(),
    };

    // Mark the chat as inactive so the user sees it's dead. We do this even
    // for blocked chats so they stay inactive if the user later unblocks the
    // contact.
    if let Some(mut chat) = crate::Chat::load_by_group_id(&mut *txn, group_id).await?
        && !matches!(chat.status(), crate::ChatStatus::Inactive(_))
    {
        chat.set_inactive(&mut *txn, past_members).await?;
    }

    // Remove any pending resync for this group (FK is on chat_id, not
    // group_id, so it won't cascade from Group::delete_from_db).
    Resync::remove(&mut *txn, group_id).await?;

    // Delete the MLS group. This cascades to pending_chat_operation and
    // group_membership via FK.
    Group::delete_from_db(txn, group_id).await?;

    Ok(())
}

#[cfg(feature = "test_utils")]
mod test_utils {
    use chrono::{DateTime, Utc};

    use crate::{Chat, ChatId, clients::CoreUser};

    impl CoreUser {
        pub async fn self_updated_at(
            &self,
            chat_id: ChatId,
        ) -> sqlx::Result<Option<DateTime<Utc>>> {
            Chat::self_updated_at(self.db().read().await?, chat_id).await
        }

        pub async fn set_self_updated_at(
            &self,
            chat_id: ChatId,
            self_updated_at: DateTime<Utc>,
        ) -> sqlx::Result<()> {
            Chat::set_self_updated_at(self.db().write().await?, chat_id, self_updated_at).await
        }
    }
}

#[cfg(test)]
mod handle_group_not_found_tests {
    use aircommon::{
        credentials::test_utils::create_test_credentials,
        identifiers::{QualifiedGroupId, UserId},
    };
    use sqlx::{Connection, query};
    use uuid::Uuid;

    use crate::{
        Chat, ChatAttributes, ChatStatus, clients::block_contact::BlockedContact,
        groups::GroupDataBytes, store::StoreNotifier, utils::persistence::open_db_in_memory,
    };

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn handle_group_not_found_marks_blocked_chat_inactive_under_block() -> anyhow::Result<()>
    {
        let pool = open_db_in_memory().await?;
        let db = DbAccess::for_tests(pool);
        let connection = db.write().await?;

        let own_user_id = UserId::random("example.com".parse().unwrap());
        let blocked_user_id = UserId::random("example.com".parse().unwrap());
        let (_as_signing_key, client_signing_key) = create_test_credentials(own_user_id);

        let qgid = QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse().unwrap());
        let group_id = GroupId::from(qgid);

        let (group, _) = Group::create_group(
            &mut connection,
            &client_signing_key,
            IdentityLinkWrapperKey::random()?,
            group_id.clone(),
            GroupDataBytes::from(b"test-group-data".to_vec()),
        )?;
        group.store(&mut *connection).await?;

        // XXX: fixme
        // let mut notifier = StoreNotifier::noop();
        let chat = Chat::new_targeted_message_chat(
            group_id.clone(),
            ChatAttributes::new("Blocked chat".into(), None),
            blocked_user_id.clone(),
        );
        let chat_id = chat.id();
        chat.store(&mut connection).await?;

        BlockedContact::new(blocked_user_id.clone())
            .store(&mut connection)
            .await?;

        assert!(matches!(
            Chat::load(&mut connection, &chat_id)
                .await?
                .expect("chat should exist")
                .status(),
            ChatStatus::Blocked
        ));

        let mut txn = connection.begin().await?;
        handle_group_not_found_on_ds(txn, &group_id).await?;
        txn.commit().await?;

        assert!(Group::load(&mut connection, &group_id).await?.is_none());
        assert!(matches!(
            Chat::load(&mut connection, &chat_id)
                .await?
                .expect("chat should still exist")
                .status(),
            ChatStatus::Blocked
        ));

        query("DELETE FROM blocked_contact WHERE user_uuid = ?1 AND user_domain = ?2")
            .bind(blocked_user_id.uuid())
            .bind(blocked_user_id.domain().to_string())
            .execute(&mut *connection)
            .await?;

        assert!(matches!(
            Chat::load(&mut connection, &chat_id)
                .await?
                .expect("chat should still exist after unblock")
                .status(),
            ChatStatus::Inactive(_)
        ));

        Ok(())
    }
}

impl TimestampedMessage {
    /// Turn a staged commit into a list of messages based on the proposals it
    /// includes.
    fn from_staged_commit(
        group: &Group,
        verified: &impl GroupStorageWitness,
        staged_commit: &StagedCommit,
        ds_timestamp: TimeStamp,
    ) -> Result<Vec<Self>> {
        // Collect the remover/removed pairs into a set to avoid duplicates.
        let mut removed_set = HashSet::new();
        let remove_proposals = staged_commit.queued_proposals().filter(|&p| {
            matches!(
                p.proposal().proposal_type(),
                ProposalType::Remove | ProposalType::SelfRemove
            )
        });
        for remove_proposal in remove_proposals {
            let sender_index = match remove_proposal.sender() {
                Sender::Member(leaf_node_index) => leaf_node_index,
                Sender::External(_) | Sender::NewMemberProposal => {
                    bail!("Only member proposals are supported for now")
                }
                Sender::NewMemberCommit => {
                    // This can only happen if the removed member is rejoining
                    // as part of the commit. No need to create a message.
                    continue;
                }
            };

            let remover = group
                .credential_at(*sender_index, verified)?
                .context("Could not find client credential of message sender")?
                .user_id()
                .clone();

            let Some(removed_index) = removed_client(remove_proposal) else {
                // This cannot happen since we filtered for remove proposals.
                continue;
            };

            let removed = group
                .credential_at(removed_index, verified)?
                .context("Could not find client credential of removed")?
                .user_id()
                .clone();

            if remover == removed {
                // A system message for this proposal was already made when it was proposed
                continue;
            }

            removed_set.insert((remover, removed));
        }
        let remove_messages = removed_set.into_iter().map(|(remover, removed)| {
            TimestampedMessage::system_message(
                SystemMessage::Remove(remover, removed),
                ds_timestamp,
            )
        });

        // Collect adder and addee names and filter out duplicates
        let mut adds_set = HashSet::new();
        for staged_add_proposal in staged_commit.add_proposals() {
            let Sender::Member(sender_index) = staged_add_proposal.sender() else {
                // We don't support non-member adds.
                bail!("Non-member add proposal")
            };
            // Get the user id of the sender from the MLS group member credential
            let sender_id = group
                .credential_at(*sender_index, verified)?
                .context("Could not find client credential of sender")?
                .user_id()
                .clone();

            // Get the user id of the added member from the proposal key package
            let credential = staged_add_proposal
                .add_proposal()
                .key_package()
                .leaf_node()
                .credential();
            let credential = VerifiableClientCredential::from_basic_credential(credential)?;
            let addee_id = credential.user_id().clone();

            adds_set.insert((sender_id, addee_id));
        }
        let add_messages = adds_set.into_iter().map(|(adder, addee)| {
            TimestampedMessage::system_message(SystemMessage::Add(adder, addee), ds_timestamp)
        });

        let event_messages = remove_messages.chain(add_messages).collect();

        // Emit log messages for updates.
        for staged_update_proposal in staged_commit.update_proposals() {
            let Sender::Member(sender_index) = staged_update_proposal.sender() else {
                // Update proposals have to be sent by group members.
                bail!("Invalid proposal")
            };
            if enabled!(Level::DEBUG) {
                let credential = group
                    .credential_at(*sender_index, verified)?
                    .context("Could not find client credential of sender")?;
                let user_id = credential.user_id();
                debug!(
                    ?user_id,
                    %sender_index, "Client has updated their key material",
                );
            }
        }

        Ok(event_messages)
    }
}

/// Returns true if the QS should suppress notifications for this message.
pub fn suppress_notifications(content: &MimiContent) -> bool {
    if content.is_status_update() {
        // Status updates should never trigger notifications.
        return true;
    }
    if content.replaces.is_some() {
        // Replaces indicates an edit or a deletion, which should not
        // trigger notifications.
        return true;
    }
    // All other messages should trigger notifications.
    false
}

fn to_capabilities_mismatch(error: CreateCommitError) -> anyhow::Result<LeafNodeValidationError> {
    use LeafNodeValidationError::*;
    match error {
        CreateCommitError::LeafNodeValidation(error)
        | CreateCommitError::ProposalValidationError(
            ProposalValidationError::LeafNodeValidation(error),
        ) if matches!(
            error,
            UnsupportedExtensions
                | UnsupportedProposals
                | UnsupportedCredentials
                | CiphersuiteNotInCapabilities
                | CredentialNotInCapabilities
                | ExtensionsNotInCapabilities
                | LeafNodeCredentialNotSupportedByMember
                | MemberCredentialNotSupportedByLeafNode,
        ) =>
        {
            Ok(error)
        }
        other => Err(other.into()),
    }
}

#[cfg(test)]
mod tests {
    use aircommon::mls_group_config::{AIR_COMPONENT_ID, default_app_data_dictionary_extension};
    use mls_assist::components::ComponentsList;
    use openmls::{
        component::ComponentType,
        prelude::{AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions, LeafNode},
    };
    use tls_codec::{DeserializeBytes, Serialize as TlsSerializeTrait};

    use super::Group;

    fn air_component_ids(params_extensions: &Extensions<LeafNode>) -> Option<Vec<u16>> {
        params_extensions
            .app_data_dictionary()?
            .dictionary()
            .get(&ComponentType::AppComponents.into())
            .and_then(|data| ComponentsList::tls_deserialize_exact_bytes(data).ok())
            .map(|list| list.component_ids)
    }

    fn extensions_with_dict(dict: AppDataDictionary) -> Extensions<LeafNode> {
        Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dict),
        )])
        .expect("valid extensions")
    }

    /// No app data dictionary -> add the default one containing AIR_COMPONENT_ID
    #[test]
    fn no_app_data_dictionary() {
        let extensions = Extensions::empty();
        let params = Group::update_leaf_node_extensions(&extensions).unwrap();
        let ids = air_component_ids(params.extensions().unwrap()).unwrap();
        assert!(ids.contains(&AIR_COMPONENT_ID));
    }

    /// App data dictionary present but no AppComponents key -> add AppComponents with AIR_COMPONENT_ID
    #[test]
    fn app_data_dictionary_without_app_components() {
        let extensions = extensions_with_dict(AppDataDictionary::new());
        let params = Group::update_leaf_node_extensions(&extensions).unwrap();
        let ids = air_component_ids(params.extensions().unwrap()).unwrap();
        assert!(ids.contains(&AIR_COMPONENT_ID));
    }

    /// AppComponents present but AIR_COMPONENT_ID missing -> add it
    #[test]
    fn app_components_without_air_component_id() {
        let other_id: u16 = 0x0001;
        let mut dict = AppDataDictionary::new();
        dict.insert(
            ComponentType::AppComponents.into(),
            ComponentsList {
                component_ids: vec![other_id],
            }
            .tls_serialize_detached()
            .unwrap(),
        );
        let extensions = extensions_with_dict(dict);
        let params = Group::update_leaf_node_extensions(&extensions).unwrap();
        let ids = air_component_ids(params.extensions().unwrap()).unwrap();
        assert!(ids.contains(&AIR_COMPONENT_ID));
        assert!(ids.contains(&other_id));
    }

    /// AIR_COMPONENT_ID already present -> extensions in params are unchanged (None)
    #[test]
    fn app_components_with_air_component_id_already() {
        let extensions = Extensions::from_vec(vec![default_app_data_dictionary_extension()])
            .expect("valid extensions");
        let params = Group::update_leaf_node_extensions(&extensions).unwrap();
        assert!(params.extensions().is_none());
    }
}
