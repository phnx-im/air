// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod client_auth_info;
// TODO: Allowing dead code here for now. We'll need diffs when we start
// rotating keys.
#[allow(dead_code)]
pub(crate) mod diff;
pub(crate) mod error;
pub(crate) mod openmls_provider;
pub(crate) mod persistence;
pub(crate) mod process;

pub(crate) use error::*;

use anyhow::{Result, anyhow, bail};
use mimi_content::MimiContent;
use mimi_room_policy::{MimiProposal, RoleIndex, RoomPolicy, VerifiedRoomState};
use mls_assist::messages::AssistedMessageOut;
use openmls_provider::PhnxOpenMlsProvider;
use openmls_traits::storage::StorageProvider;
use phnxtypes::{
    credentials::{ClientCredential, keys::ClientSigningKey},
    crypto::{
        ear::{
            EarDecryptable, EarEncryptable,
            keys::{
                EncryptedUserProfileKey, GroupStateEarKey, IdentityLinkWrapperKey,
                WelcomeAttributionInfoEarKey,
            },
        },
        hpke::{HpkeDecryptable, JoinerInfoDecryptionKey},
        indexed_aead::keys::UserProfileKey,
        signatures::signable::{Signable, Verifiable},
    },
    identifiers::{QS_CLIENT_REFERENCE_EXTENSION_TYPE, QsReference, QualifiedGroupId, UserId},
    messages::{
        client_ds::{
            DsJoinerInformation, GroupOperationParamsAad, InfraAadMessage, InfraAadPayload,
            UpdateParamsAad, WelcomeBundle,
        },
        client_ds_out::{
            AddUsersInfoOut, CreateGroupParamsOut, DeleteGroupParamsOut, ExternalCommitInfoIn,
            GroupOperationParamsOut, SelfRemoveParamsOut, SendMessageParamsOut, UpdateParamsOut,
            WelcomeInfoIn,
        },
        welcome_attribution_info::{
            WelcomeAttributionInfo, WelcomeAttributionInfoPayload, WelcomeAttributionInfoTbs,
        },
    },
    time::TimeStamp,
};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqliteExecutor, SqliteTransaction};
use tracing::{debug, error};

use crate::{
    SystemMessage, clients::api_clients::ApiClients, contacts::ContactAddInfos,
    conversations::messages::TimestampedMessage,
};
use std::collections::HashSet;

use openmls::{
    group::ProcessedWelcome,
    key_packages::KeyPackageBundle,
    prelude::{
        Capabilities, Ciphersuite, CredentialType, CredentialWithKey, Extension, ExtensionType,
        Extensions, GroupId, KeyPackage, LeafNodeIndex, MlsGroup, MlsGroupJoinConfig,
        MlsMessageOut, OpenMlsProvider, PURE_PLAINTEXT_WIRE_FORMAT_POLICY, Proposal, ProposalType,
        ProtocolVersion, QueuedProposal, RequiredCapabilitiesExtension, Sender, StagedCommit,
        UnknownExtension, tls_codec::Serialize as TlsSerializeTrait,
    },
    treesync::RatchetTree,
};

use self::{
    client_auth_info::{ClientAuthInfo, GroupMembership, StorableClientCredential},
    diff::StagedGroupDiff,
};

pub const FRIENDSHIP_PACKAGE_PROPOSAL_TYPE: u16 = 0xff00;
pub const GROUP_DATA_EXTENSION_TYPE: u16 = 0xff01;

pub const DEFAULT_MLS_VERSION: ProtocolVersion = ProtocolVersion::Mls10;
pub const DEFAULT_CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

pub const REQUIRED_EXTENSION_TYPES: [ExtensionType; 3] = [
    ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE),
    ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE),
    ExtensionType::LastResort,
];
pub const REQUIRED_PROPOSAL_TYPES: [ProposalType; 1] =
    [ProposalType::Custom(FRIENDSHIP_PACKAGE_PROPOSAL_TYPE)];
pub const REQUIRED_CREDENTIAL_TYPES: [CredentialType; 1] = [CredentialType::Basic];

pub fn default_required_capabilities() -> RequiredCapabilitiesExtension {
    RequiredCapabilitiesExtension::new(
        &REQUIRED_EXTENSION_TYPES,
        &REQUIRED_PROPOSAL_TYPES,
        &REQUIRED_CREDENTIAL_TYPES,
    )
}

// Default capabilities for every leaf node we create.
pub const SUPPORTED_PROTOCOL_VERSIONS: [ProtocolVersion; 1] = [DEFAULT_MLS_VERSION];
pub const SUPPORTED_CIPHERSUITES: [Ciphersuite; 1] = [DEFAULT_CIPHERSUITE];
pub const SUPPORTED_EXTENSIONS: [ExtensionType; 3] = REQUIRED_EXTENSION_TYPES;
pub const SUPPORTED_PROPOSALS: [ProposalType; 1] = REQUIRED_PROPOSAL_TYPES;
pub const SUPPORTED_CREDENTIALS: [CredentialType; 1] = REQUIRED_CREDENTIAL_TYPES;

pub fn default_capabilities() -> Capabilities {
    Capabilities::new(
        Some(&SUPPORTED_PROTOCOL_VERSIONS),
        Some(&SUPPORTED_CIPHERSUITES),
        Some(&SUPPORTED_EXTENSIONS),
        Some(&SUPPORTED_PROPOSALS),
        Some(&SUPPORTED_CREDENTIALS),
    )
}

pub(crate) struct PartialCreateGroupParams {
    pub(crate) group_id: GroupId,
    ratchet_tree: RatchetTree,
    group_info: MlsMessageOut,
    room_state: VerifiedRoomState,
}

impl PartialCreateGroupParams {
    pub(crate) fn into_params(
        self,
        client_reference: QsReference,
        encrypted_user_profile_key: EncryptedUserProfileKey,
    ) -> CreateGroupParamsOut {
        CreateGroupParamsOut {
            group_id: self.group_id,
            ratchet_tree: self.ratchet_tree,
            encrypted_user_profile_key,
            creator_client_reference: client_reference,
            group_info: self.group_info,
            room_state: serde_json::to_vec(&self.room_state).unwrap(),
        }
    }
}

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

pub(crate) struct GroupData {
    bytes: Vec<u8>,
}

impl GroupData {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl From<Vec<u8>> for GroupData {
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
}

impl Group {
    pub(crate) fn mls_group(&self) -> &MlsGroup {
        &self.mls_group
    }

    fn default_mls_group_join_config() -> MlsGroupJoinConfig {
        MlsGroupJoinConfig::builder()
            .wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .build()
    }

    /// Create a group.
    pub(super) fn create_group(
        provider: &impl OpenMlsProvider,
        signer: &ClientSigningKey,
        group_id: GroupId,
        group_data: GroupData,
    ) -> Result<(Self, GroupMembership, PartialCreateGroupParams)> {
        let group_state_ear_key = GroupStateEarKey::random()?;
        let identity_link_wrapper_key = IdentityLinkWrapperKey::random()?;

        let required_capabilities =
            Extension::RequiredCapabilities(default_required_capabilities());
        let leaf_node_capabilities = default_capabilities();

        let group_data_extension = Extension::Unknown(
            GROUP_DATA_EXTENSION_TYPE,
            UnknownExtension(group_data.bytes),
        );
        let gc_extensions =
            Extensions::from_vec(vec![group_data_extension, required_capabilities])?;

        let credential_with_key = CredentialWithKey {
            credential: signer.credential().try_into()?,
            signature_key: signer.credential().verifying_key().clone().into(),
        };

        let mls_group = MlsGroup::builder()
            .with_group_id(group_id.clone())
            .with_capabilities(leaf_node_capabilities)
            .with_group_context_extensions(gc_extensions)?
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .build(provider, signer, credential_with_key)
            .map_err(|e| anyhow!("Error while creating group: {:?}", e))?;

        let room_state = VerifiedRoomState::new(&0, RoomPolicy::default_private()).unwrap();

        let params = PartialCreateGroupParams {
            group_id: group_id.clone(),
            ratchet_tree: mls_group.export_ratchet_tree(),
            group_info: mls_group.export_group_info(provider, signer, true)?,
            room_state: room_state.clone(),
        };

        let group_membership = GroupMembership::new(
            signer.credential().identity().clone(),
            group_id.clone(),
            LeafNodeIndex::new(0), // We just created the group so we're at index 0.
            signer.credential().fingerprint(),
        );

        let group = Self {
            group_id,
            identity_link_wrapper_key,
            mls_group,
            room_state,
            group_state_ear_key: group_state_ear_key.clone(),
            pending_diff: None,
        };

        Ok((group, group_membership, params))
    }

    /// Join a group with the provided welcome message. If there exists a group
    /// with the same ID, checks if that group is inactive and if so deletes the
    /// old group.
    ///
    /// Returns the group name.
    pub(super) async fn join_group(
        welcome_bundle: WelcomeBundle,
        // This is our own key that the sender uses to encrypt to us. We should
        // be able to retrieve it from the client's key store.
        welcome_attribution_info_ear_key: &WelcomeAttributionInfoEarKey,
        txn: &mut SqliteTransaction<'_>,
        api_clients: &ApiClients,
        signer: &ClientSigningKey,
    ) -> Result<(Self, Vec<ProfileInfo>)> {
        let serialized_welcome = welcome_bundle.welcome.tls_serialize_detached()?;

        let mls_group_config = Self::default_mls_group_join_config();

        let (processed_welcome, joiner_info) = {
            // Phase 1: Fetch the right KeyPackageBundle from storage
            let provider = PhnxOpenMlsProvider::new(txn.as_mut());
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
            if let Some(group) = Self::load(txn.as_mut(), &group_id).await? {
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

        let (mls_group, joiner_info, welcome_attribution_info) = {
            // Phase 5: Finish processing the welcome message
            let provider = PhnxOpenMlsProvider::new(txn.as_mut());
            let staged_welcome =
                processed_welcome.into_staged_welcome(&provider, Some(ratchet_tree))?;

            let mls_group = staged_welcome.into_group(&provider)?;

            // Decrypt WelcomeAttributionInfo
            let verifiable_attribution_info = WelcomeAttributionInfo::decrypt(
                welcome_attribution_info_ear_key,
                &welcome_bundle.encrypted_attribution_info,
            )?
            .into_verifiable(mls_group.group_id().clone(), serialized_welcome);

            let sender_user_id = verifiable_attribution_info.sender();
            let sender_client_credential =
                StorableClientCredential::load_by_user_id(txn.as_mut(), &sender_user_id)
                    .await?
                    .ok_or_else(|| {
                        anyhow!("Could not find client credential of sender in database.")
                    })?;

            let welcome_attribution_info: WelcomeAttributionInfoPayload =
                verifiable_attribution_info.verify(sender_client_credential.verifying_key())?;

            (mls_group, joiner_info, welcome_attribution_info)
        };

        let client_information = mls_group.members().map(|m| (m.index, m.credential));

        // Phase 6: Decrypt and verify the client credentials. This can involve
        // queries to the clients' AS.
        let client_information = ClientAuthInfo::verify_credentials(
            txn,
            api_clients,
            mls_group.group_id(),
            client_information,
        )
        .await?;

        // Phase 7: Decrypt and verify the infra credentials.
        {
            for client_auth_info in &client_information {
                client_auth_info.store(txn).await?;
            }
        }

        let member_profile_info = encrypted_user_profile_keys
            .into_iter()
            .zip(
                client_information
                    .into_iter()
                    .map(|client_auth_info| client_auth_info.into_client_credential()),
            )
            .map(|(eupk, ci)| {
                UserProfileKey::decrypt(
                    welcome_attribution_info.identity_link_wrapper_key(),
                    &eupk,
                    ci.identity(),
                )
                .map(|user_profile_key| ProfileInfo {
                    user_profile_key,
                    client_credential: ci,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let group = Self {
            group_id: mls_group.group_id().clone(),
            mls_group,
            identity_link_wrapper_key: welcome_attribution_info.identity_link_wrapper_key().clone(),
            group_state_ear_key: joiner_info.group_state_ear_key,
            pending_diff: None,
            room_state: serde_json::from_slice(&room_state).unwrap(),
        };

        Ok((group, member_profile_info))
    }

    /// Join a group using an external commit.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn join_group_externally(
        connection: &mut SqliteConnection,
        api_clients: &ApiClients,
        external_commit_info: ExternalCommitInfoIn,
        signer: &ClientSigningKey,
        group_state_ear_key: GroupStateEarKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        aad: InfraAadMessage,
        own_client_credential: &ClientCredential,
    ) -> Result<(Self, MlsMessageOut, MlsMessageOut, Vec<ProfileInfo>)> {
        // TODO: We set the ratchet tree extension for now, as it is the only
        // way to make OpenMLS return a GroupInfo. This should change in the
        // future.
        let mls_group_config = Self::default_mls_group_join_config();
        let credential_with_key = CredentialWithKey {
            credential: signer.credential().try_into()?,
            signature_key: signer.credential().verifying_key().clone().into(),
        };
        let ExternalCommitInfoIn {
            verifiable_group_info,
            ratchet_tree_in,
            encrypted_user_profile_keys,
            room_state,
        } = external_commit_info;

        // Let's create the group first so that we can access the GroupId.
        // Phase 1: Create and store the group
        let (mls_group, commit, group_info) = {
            let provider = PhnxOpenMlsProvider::new(&mut *connection);
            let (mut mls_group, commit, _) = MlsGroup::join_by_external_commit(
                &provider,
                signer,
                Some(ratchet_tree_in),
                verifiable_group_info,
                &mls_group_config,
                Some(default_capabilities()),
                None,
                &aad.tls_serialize_detached()?,
                credential_with_key,
            )?;
            mls_group.merge_pending_commit(&provider)?;
            let group_info = mls_group.export_group_info(&provider, signer, true)?;
            (mls_group, commit, group_info)
        };

        let group_id = mls_group.group_id();

        let client_credentials = mls_group.members().map(|m| (m.index, m.credential));

        // Phase 2: Decrypt and verify the client credentials.
        let mut client_information = ClientAuthInfo::verify_credentials(
            &mut *connection,
            api_clients,
            group_id,
            client_credentials,
        )
        .await?;

        // Compile a list of user profile keys for the members.
        let member_profile_info = encrypted_user_profile_keys
            .into_iter()
            .zip(
                client_information
                    .iter()
                    .map(|client_auth_info| client_auth_info.client_credential()),
            )
            .map(|(eupk, ci)| {
                UserProfileKey::decrypt(&identity_link_wrapper_key, &eupk, ci.identity()).map(
                    |user_profile_key| ProfileInfo {
                        user_profile_key,
                        client_credential: ci.clone().into(),
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        // We still have to add ourselves to the encrypted client credentials.
        let own_index = mls_group.own_leaf_index().usize();
        let own_group_membership = GroupMembership::new(
            own_client_credential.identity().clone(),
            mls_group.group_id().clone(),
            LeafNodeIndex::new(own_index as u32),
            own_client_credential.fingerprint(),
        );

        let own_auth_info =
            ClientAuthInfo::new(own_client_credential.clone(), own_group_membership);
        client_information.push(own_auth_info);

        // Phase 3: Verify and store the infra credentials.
        {
            for client_auth_info in client_information.iter() {
                // Store client auth info.
                client_auth_info.store(&mut *connection).await?;
            }
        }

        let group = Self {
            group_id: mls_group.group_id().clone(),
            mls_group,
            identity_link_wrapper_key,
            group_state_ear_key,
            pending_diff: None,
            room_state: serde_json::from_slice(&room_state).unwrap(),
        };

        Ok((group, commit, group_info, member_profile_info))
    }

    /// Invite the given list of contacts to join the group.
    ///
    /// Returns the [`AddUserParamsOut`] as input for the API client.
    pub(super) async fn stage_invite(
        &mut self,
        connection: &mut SqliteConnection,
        signer: &ClientSigningKey,
        // The following three vectors have to be in sync, i.e. of the same length
        // and refer to the same contacts in order.
        add_infos: Vec<ContactAddInfos>,
        wai_keys: Vec<WelcomeAttributionInfoEarKey>,
        client_credentials: Vec<ClientCredential>,
    ) -> Result<GroupOperationParamsOut> {
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
                upk.encrypt(
                    &self.identity_link_wrapper_key,
                    client_credential.identity(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let aad_message: InfraAadMessage =
            InfraAadPayload::GroupOperation(GroupOperationParamsAad {
                new_encrypted_user_profile_keys,
                credential_update_option: None,
            })
            .into();

        // Set Aad to contain the encrypted client credentials.
        let (mls_commit, welcome_option, group_info_option) = {
            let provider = PhnxOpenMlsProvider::new(&mut *connection);
            self.mls_group
                .set_aad(aad_message.tls_serialize_detached()?);
            self.mls_group
                .commit_builder()
                .force_self_update(true)
                .create_group_info(true)
                .propose_adds(key_packages)
                .load_psks(provider.storage())?
                .build(provider.rand(), provider.crypto(), signer, |_| true)?
                .stage_commit(&provider)?
                .into_contents()
        };

        let group_info = group_info_option.ok_or(anyhow!("Commit didn't return a group info"))?;
        let welcome = MlsMessageOut::from_welcome(
            welcome_option.ok_or(anyhow!("Commit didn't return a welcome"))?,
            ProtocolVersion::default(),
        );
        let commit = AssistedMessageOut::new(mls_commit, Some(group_info.into()))?;

        let encrypted_welcome_attribution_infos = wai_keys
            .iter()
            .map(|wai_key| {
                // WAI = WelcomeAttributionInfo
                let wai_payload = WelcomeAttributionInfoPayload::new(
                    signer.credential().identity().clone(),
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
            .collect::<Result<Vec<_>>>()?;

        // Stage removals
        for remove in self
            .mls_group
            .pending_commit()
            .ok_or(anyhow!("No pending commit after commit operation"))?
            .remove_proposals()
        {
            GroupMembership::stage_removal(
                &mut *connection,
                self.group_id(),
                remove.remove_proposal().removed(),
            )
            .await?;
        }

        // Stage the adds in the DB.
        let free_indices = GroupMembership::free_indices(&mut *connection, self.group_id()).await?;
        for (leaf_index, client_credential) in free_indices.zip(client_credentials) {
            // Room policy check
            self.room_state.apply_regular_proposals(
                &self.mls_group.own_leaf_index().u32(),
                &[MimiProposal::ChangeRole {
                    target: leaf_index.u32(),
                    role: RoleIndex::Regular,
                }],
            )?;

            let fingerprint = client_credential.fingerprint();
            let group_membership = GroupMembership::new(
                client_credential.identity().clone(),
                self.group_id.clone(),
                leaf_index,
                fingerprint,
            );
            let client_auth_info = ClientAuthInfo::new(client_credential, group_membership);
            client_auth_info.stage_add(&mut *connection).await?;
        }

        let add_users_info = AddUsersInfoOut {
            welcome,
            encrypted_welcome_attribution_infos,
        };

        let params = GroupOperationParamsOut {
            commit,
            add_users_info_option: Some(add_users_info),
        };

        Ok(params)
    }

    pub(super) async fn stage_remove(
        &mut self,
        connection: &mut sqlx::SqliteConnection,
        signer: &ClientSigningKey,
        members: Vec<UserId>,
    ) -> Result<GroupOperationParamsOut> {
        let remove_indices =
            GroupMembership::client_indices(&mut *connection, self.group_id(), &members).await?;

        // Room policy checks
        for client in &remove_indices {
            self.room_state.apply_regular_proposals(
                &self.mls_group.own_leaf_index().u32(),
                &[MimiProposal::ChangeRole {
                    target: client.u32(),
                    role: RoleIndex::Outsider,
                }],
            )?;
        }

        let aad_payload = InfraAadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys: vec![],
            credential_update_option: None,
        });
        let aad = InfraAadMessage::from(aad_payload).tls_serialize_detached()?;
        self.mls_group.set_aad(aad);
        let provider = PhnxOpenMlsProvider::new(&mut *connection);

        let (mls_message, _welcome_option, group_info_option) = self
            .mls_group
            .commit_builder()
            .force_self_update(true)
            .create_group_info(true)
            .propose_removals(remove_indices)
            .load_psks(provider.storage())?
            .build(provider.rand(), provider.crypto(), signer, |_| true)?
            .stage_commit(&provider)?
            .into_contents();

        // There shouldn't be a welcome
        debug_assert!(_welcome_option.is_none());
        let group_info = group_info_option.ok_or(anyhow!("No group info after commit"))?;
        let commit = AssistedMessageOut::new(mls_message, Some(group_info.into()))?;

        for remove in self
            .mls_group()
            .pending_commit()
            .ok_or(anyhow!("No pending commit after commit operation"))?
            .remove_proposals()
        {
            GroupMembership::stage_removal(
                &mut *connection,
                self.group_id(),
                remove.remove_proposal().removed(),
            )
            .await?;
        }

        let params = GroupOperationParamsOut {
            commit,
            add_users_info_option: None,
        };
        Ok(params)
    }

    pub(super) async fn stage_delete(
        &mut self,
        connection: &mut sqlx::SqliteConnection,
        signer: &ClientSigningKey,
    ) -> anyhow::Result<DeleteGroupParamsOut> {
        let provider = &PhnxOpenMlsProvider::new(&mut *connection);
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
        let aad_payload = InfraAadPayload::DeleteGroup;
        let aad = InfraAadMessage::from(aad_payload).tls_serialize_detached()?;
        self.mls_group.set_aad(aad);

        let (mls_message, _welcome_option, group_info_option) = self
            .mls_group
            .commit_builder()
            .force_self_update(true)
            .create_group_info(true)
            .propose_removals(remove_indices)
            .load_psks(provider.storage())?
            .build(provider.rand(), provider.crypto(), signer, |_| true)?
            .stage_commit(provider)?
            .into_contents();

        debug_assert!(_welcome_option.is_none());
        let group_info =
            group_info_option.ok_or(anyhow!("No group info after commit operation"))?;
        let commit = AssistedMessageOut::new(mls_message, Some(group_info.into()))?;

        for remove in self
            .mls_group()
            .pending_commit()
            .ok_or(anyhow!("No pending commit after commit operation"))?
            .remove_proposals()
        {
            GroupMembership::stage_removal(
                &mut *connection,
                self.group_id(),
                remove.remove_proposal().removed(),
            )
            .await?;
        }

        let params = DeleteGroupParamsOut { commit };
        Ok(params)
    }

    /// If a [`StagedCommit`] is given, merge it and apply the pending group
    /// diff. If no [`StagedCommit`] is given, merge any pending commit and
    /// apply the pending group diff.
    pub(super) async fn merge_pending_commit(
        &mut self,
        connection: &mut sqlx::SqliteConnection,
        staged_commit_option: impl Into<Option<StagedCommit>>,
        ds_timestamp: TimeStamp,
    ) -> Result<Vec<TimestampedMessage>> {
        let free_indices = GroupMembership::free_indices(&mut *connection, self.group_id()).await?;
        let staged_commit_option: Option<StagedCommit> = staged_commit_option.into();

        let event_messages = if let Some(staged_commit) = staged_commit_option {
            // Compute the messages we want to emit from the staged commit and the
            // client info diff.
            let staged_commit_messages = TimestampedMessage::from_staged_commit(
                &mut *connection,
                self.group_id(),
                free_indices,
                &staged_commit,
                ds_timestamp,
            )
            .await?;

            let provider = PhnxOpenMlsProvider::new(&mut *connection);
            self.mls_group
                .merge_staged_commit(&provider, staged_commit)?;
            staged_commit_messages
        } else {
            // If we're merging a pending commit, we need to check if we have
            // committed a remove proposal by reference. If we have, we need to
            // create a notification message.
            let staged_commit_messages =
                if let Some(staged_commit) = self.mls_group.pending_commit() {
                    TimestampedMessage::from_staged_commit(
                        &mut *connection,
                        self.group_id(),
                        free_indices,
                        staged_commit,
                        ds_timestamp,
                    )
                    .await?
                } else {
                    vec![]
                };
            let provider = PhnxOpenMlsProvider::new(&mut *connection);
            self.mls_group.merge_pending_commit(&provider)?;
            staged_commit_messages
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

        GroupMembership::merge_for_group(&mut *connection, self.group_id()).await?;
        self.pending_diff = None;
        // Debug sanity checks after merging.
        #[cfg(debug_assertions)]
        {
            let mls_group_members = self
                .mls_group
                .members()
                .map(|m| m.index)
                .collect::<Vec<_>>();
            let infra_group_members =
                GroupMembership::group_members(&mut *connection, self.group_id()).await?;
            if mls_group_members.len() != infra_group_members.len() {
                tracing::info!(?mls_group_members, "Group members according to OpenMLS");
                tracing::info!(?infra_group_members, "Group members according to Infra");
                panic!("Group members don't match up");
            }
            let infra_indices = GroupMembership::client_indices(
                &mut *connection,
                self.group_id(),
                &infra_group_members,
            )
            .await?;
            self.mls_group.members().for_each(|m| {
                let index = m.index;
                debug_assert!(infra_indices.contains(&index));
            });
        }
        Ok(event_messages)
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
            .create_message(provider, signer, &content.serialize())?;

        let message = AssistedMessageOut::new(mls_message, None)?;

        let send_message_params = SendMessageParamsOut {
            sender: self.mls_group.own_leaf_index(),
            message,
        };

        Ok(send_message_params)
    }

    pub(super) fn own_leaf_index(&self) -> u32 {
        self.mls_group.own_leaf_index().u32()
    }

    /// Get a reference to the group's group id.
    pub(crate) fn group_id(&self) -> &GroupId {
        self.mls_group().group_id()
    }

    pub(crate) fn group_state_ear_key(&self) -> &GroupStateEarKey {
        &self.group_state_ear_key
    }

    pub async fn client_by_index(
        &self,
        connection: &mut sqlx::SqliteConnection,
        index: LeafNodeIndex,
    ) -> Option<UserId> {
        GroupMembership::load(&mut *connection, self.group_id(), index)
            .await
            .ok()
            .flatten()
            .map(|group_membership| group_membership.user_id().clone())
    }

    pub(crate) fn identity_link_wrapper_key(&self) -> &IdentityLinkWrapperKey {
        &self.identity_link_wrapper_key
    }

    /// Returns a set containing the [`UserId`] of the members of the group.
    pub(crate) async fn members(&self, executor: impl SqliteExecutor<'_>) -> HashSet<UserId> {
        match GroupMembership::group_members(executor, self.group_id()).await {
            // deduplicate by collecting into as set
            Ok(group_members) => group_members.into_iter().collect(),
            Err(error) => {
                error!(%error, "Could not retrieve group members");
                HashSet::new()
            }
        }
    }

    pub(super) async fn update(
        &mut self,
        txn: &mut SqliteTransaction<'_>,
        signer: &ClientSigningKey,
    ) -> Result<UpdateParamsOut> {
        // We don't expect there to be a welcome.
        let aad_payload = UpdateParamsAad {}; // TODO: Do we still need this struct?

        let aad =
            InfraAadMessage::from(InfraAadPayload::Update(aad_payload)).tls_serialize_detached()?;
        self.mls_group.set_aad(aad);
        let (mls_message, group_info) = {
            let provider = PhnxOpenMlsProvider::new(txn.as_mut());

            let (mls_message, _welcome_option, group_info_option) = self
                .mls_group
                .commit_builder()
                .force_self_update(true)
                .create_group_info(true)
                .load_psks(provider.storage())?
                .build(provider.rand(), provider.crypto(), signer, |_| true)?
                .stage_commit(&provider)?
                .into_contents();

            (
                mls_message,
                group_info_option.ok_or_else(|| anyhow!("No group info after commit"))?,
            )
        };

        for remove in self
            .mls_group()
            .pending_commit()
            .ok_or(anyhow!("No pending commit after commit operation"))?
            .remove_proposals()
        {
            GroupMembership::stage_removal(
                txn.as_mut(),
                self.group_id(),
                remove.remove_proposal().removed(),
            )
            .await?;
        }
        let commit = AssistedMessageOut::new(mls_message, Some(group_info.into()))?;
        Ok(UpdateParamsOut { commit })
    }

    pub(super) fn stage_leave_group(
        &mut self,
        connection: &mut sqlx::SqliteConnection,
        signer: &ClientSigningKey,
    ) -> Result<SelfRemoveParamsOut> {
        let provider = &PhnxOpenMlsProvider::new(connection);
        let proposal = self.mls_group.leave_group(provider, signer)?;

        let assisted_message = AssistedMessageOut::new(proposal, None)?;
        let params = SelfRemoveParamsOut {
            remove_proposal: assisted_message,
        };
        Ok(params)
    }

    pub(super) fn store_proposal(
        &mut self,
        connection: &mut sqlx::SqliteConnection,
        proposal: QueuedProposal,
    ) -> Result<()> {
        let provider = &PhnxOpenMlsProvider::new(connection);
        self.mls_group
            .store_pending_proposal(provider.storage(), proposal)?;
        Ok(())
    }

    pub(crate) async fn pending_removes(
        &self,
        connection: &mut sqlx::SqliteConnection,
    ) -> Vec<UserId> {
        let mut pending_removes = Vec::new();
        for proposal in self.mls_group().pending_proposals() {
            if let Proposal::Remove(rp) = proposal.proposal() {
                if let Some(client) = self.client_by_index(connection, rp.removed()).await {
                    pending_removes.push(client);
                }
            }
        }
        pending_removes
    }

    pub(crate) fn group_data(&self) -> Option<GroupData> {
        self.mls_group().extensions().iter().find_map(|e| match e {
            Extension::Unknown(GROUP_DATA_EXTENSION_TYPE, extension_bytes) => {
                Some(GroupData::from(extension_bytes.0.clone()))
            }
            _ => None,
        })
    }

    pub(crate) fn own_index(&self) -> LeafNodeIndex {
        self.mls_group().own_leaf_index()
    }
}

impl TimestampedMessage {
    /// Turn a staged commit into a list of messages based on the proposals it
    /// includes.
    async fn from_staged_commit(
        connection: &mut sqlx::SqliteConnection,
        group_id: &GroupId,
        free_indices: impl Iterator<Item = LeafNodeIndex>,
        staged_commit: &StagedCommit,
        ds_timestamp: TimeStamp,
    ) -> Result<Vec<Self>> {
        // Collect the remover/removed pairs into a set to avoid duplicates.
        let mut removed_set = HashSet::new();
        for remove_proposal in staged_commit.remove_proposals() {
            let Sender::Member(sender_index) = remove_proposal.sender() else {
                bail!("Only member proposals are supported for now")
            };
            let remover = if let Some(remover) =
                ClientAuthInfo::load(&mut *connection, group_id, *sender_index).await?
            {
                remover
            } else {
                // This is in case we removed ourselves.
                ClientAuthInfo::load_staged(&mut *connection, group_id, *sender_index)
                    .await?
                    .ok_or_else(|| anyhow!("Could not find client credential of remover"))?
            }
            .client_credential()
            .identity()
            .clone();
            let removed_index = remove_proposal.remove_proposal().removed();
            let removed = ClientAuthInfo::load_staged(connection, group_id, removed_index)
                .await?
                .ok_or_else(|| anyhow!("Could not find client credential of removed"))?
                .client_credential()
                .identity()
                .clone();
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
        for (staged_add_proposal, free_index) in staged_commit.add_proposals().zip(free_indices) {
            let Sender::Member(sender_index) = staged_add_proposal.sender() else {
                // We don't support non-member adds.
                bail!("Non-member add proposal")
            };
            // Get the name of the sender from the list of existing clients
            let sender_name = ClientAuthInfo::load(connection, group_id, *sender_index)
                .await?
                .ok_or_else(|| anyhow!("Could not find client credential of sender"))?
                .client_credential()
                .identity()
                .clone();
            // Get the name of the added member from the diff containing
            // the new clients.
            let addee_name = ClientAuthInfo::load_staged(connection, group_id, free_index)
                .await?
                .ok_or_else(|| {
                    anyhow!(
                        "Could not find client credential of added client at index {}",
                        free_index
                    )
                })?
                .client_credential()
                .identity()
                .clone();
            adds_set.insert((sender_name, addee_name));
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
            let client_auth_info = ClientAuthInfo::load(&mut *connection, group_id, *sender_index)
                .await?
                .ok_or_else(|| anyhow!("Could not find client credential of sender"))?;
            let user_id = client_auth_info.client_credential().identity();
            debug!(
                ?user_id,
                %sender_index, "Client has updated their key material",
            );
        }

        Ok(event_messages)
    }
}
