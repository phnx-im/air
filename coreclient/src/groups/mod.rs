// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod apq_group;
pub(crate) mod client_auth_info;
pub(crate) mod compatibility;
pub(crate) mod debug_info;
// TODO: Allowing dead code here for now. We'll need diffs when we start
// rotating keys.
#[allow(dead_code)]
pub(crate) mod diff;
pub(crate) mod error;
// The acting and sibling sides of multi-client group creation land separately
// and wire these helpers up.
#[cfg_attr(not(test), expect(dead_code, reason = "not yet wired up"))]
pub(crate) mod group_bootstrap;
pub(crate) mod openmls_provider;
pub(crate) mod persistence;
pub(crate) mod process;
pub(crate) mod self_group;
pub(crate) mod self_group_message_key;

use apqmls::{
    authentication::{ApqCredentialWithKey, ApqSigner},
    commit_builder::ApqCommitMessageBundle,
    extension::ApqInfo,
    external_commit_builder::{ApqExternalCommitBuilder, ApqExternalCommitBuilderError},
    messages::{ApqProposalIn, ApqRatchetTreeIn, VerifiableApqGroupInfo},
    validation::{validate_apq_session_at_construction, validate_welcome_psk},
};
pub(crate) use error::*;
pub(crate) use persistence::VerifiedGroup;

use std::collections::{HashMap, HashSet};

use aircommon::{
    credentials::{
        GroupStorageWitness, LeafCredential, LeafCredentialError, RoomPolicyIdentity,
        UserCredential, VerifiableUserCredential,
        keys::{ClientKeyType, LeafSigningKey, SelfGroupSigningKey, UserSigningKey},
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
        signatures::{
            private_keys::SigningKey,
            signable::{Signable, Verifiable},
        },
    },
    identifiers::{QsReference, QualifiedGroupId, UserId},
    messages::{
        client_as::ConnectionOfferHash,
        client_ds::{
            AadMessage, AadPayload, ApqWelcomeBundle, DsJoinerInformation, GroupOperationParamsAad,
            WelcomeBundle,
        },
        client_ds_out::{
            AddUsersInfoOut, ApqGroupOperationParamsOut, CollisionTag, CreateGroupParamsOut,
            CreatePqGroupParamsOut, DeleteGroupParamsOut, ExternalCommitInfoIn,
            GroupOperationParamsOut, PqExternalCommitInfoIn, SelfRemoveParamsOut,
            SendMessageCollisionTag, SendMessageParamsOut, TargetedMessageParamsOut,
            TargetedMessageType, WelcomeInfoIn,
        },
        welcome_attribution_info::{
            WelcomeAttributionInfo, WelcomeAttributionInfoPayload, WelcomeAttributionInfoTbs,
        },
    },
    mls_group_config::{
        AppComponent, GROUP_DATA_EXTENSION_TYPE, MAX_PAST_EPOCHS,
        default_app_data_dictionary_extension, default_group_required_extensions,
        default_leaf_node_capabilities, default_leaf_node_extensions,
        default_mls_group_join_config, default_sender_ratchet_configuration,
        leaf_node_is_virtual_client, self_group_leaf_node_capabilities, vc_leaf_node_extensions,
    },
    time::TimeStamp,
    utils::removed_client,
};
use airprotos::client::component::{
    AIR_COMPONENT_ID, AirComponent, AirFeatures, SUPPORTED_COMPONENTS,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use hkdf::Hkdf;
use mimi_content::{MessageStatus, MessageStatusReport, MimiContent, PerMessageStatus};
use mimi_room_policy::{MimiProposal, RoleIndex, RoomPolicy, VerifiedRoomState};
use mls_assist::{components::ComponentsList, messages::AssistedMessageOut};
use openmls_provider::AirOpenMlsProvider;
use openmls_traits::signatures::Signer;
use openmls_traits::storage::StorageProvider;
use serde::Serialize;
use sha2::Sha256;
use tls_codec::DeserializeBytes;
use tracing::{Level, debug, enabled, error, warn};
use uuid::Uuid;

use crate::{
    ChatId, ChatStatus, SystemMessage,
    chats::messages::TimestampedMessage,
    clients::{
        api_clients::ApiClients,
        block_contact::{BlockedContact, BlockedContactError},
        own_client_info::OwnClientInfo,
        targeted_message::TargetedMessageContent,
    },
    contacts::{ContactAddInfos, ContactKeyPackage},
    db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
    groups::{apq_group::PqGroup, client_auth_info::VerifiableUserCredentialExt},
    job::chat_operation::DerivationEpoch,
    key_stores::as_credentials::AsCredentials,
    outbound_service::resync::Resync,
};

use openmls::{
    component::ComponentType,
    components::vc_derivation_info::GenerationId,
    group::{
        CreateCommitError, ExportSecretError, ExternalCommitBuilder, GroupEpoch, JoinBuilder,
        ProcessedWelcome, ProposalValidationError, UnconfirmedMessage,
    },
    prelude::{
        AppDataDictionaryExtension, Capabilities, Credential, CredentialType, CredentialWithKey,
        Extension, Extensions, GroupId, LeafNode, LeafNodeIndex, LeafNodeParameters, MlsGroup,
        MlsMessageBodyIn, MlsMessageIn, MlsMessageOut, OpenMlsProvider,
        PURE_PLAINTEXT_WIRE_FORMAT_POLICY, PreSharedKeyProposal, Proposal, ProposalType,
        ProtocolVersion, QueuedProposal, Sender, SignaturePublicKey, StagedCommit,
        UnknownExtension, tls_codec::Serialize as TlsSerializeTrait,
    },
    schedule::{ExternalPsk, PreSharedKeyId, Psk},
    treesync::{RatchetTree, RatchetTreeIn, errors::LeafNodeValidationError},
};

use self::{client_auth_info::StorableUserCredential, diff::StagedGroupDiff};

pub(crate) struct PartialCreateGroupParams {
    pub(crate) group_id: GroupId,
    ratchet_tree: RatchetTree,
    group_info: MlsMessageOut,
    pub(crate) room_state: VerifiedRoomState,
    pq: Option<PartialPqCreateGroupParams>,
}

pub(crate) struct PartialPqCreateGroupParams {
    group_id: GroupId,
    ratchet_tree: RatchetTree,
    group_info: MlsMessageOut,
}

impl PartialCreateGroupParams {
    pub(crate) fn into_params(
        self,
        creator_client_reference: QsReference,
        encrypted_user_profile_key: EncryptedUserProfileKey,
    ) -> CreateGroupParamsOut {
        let pq = self.pq.map(|pq| CreatePqGroupParamsOut {
            group_id: pq.group_id,
            ratchet_tree: pq.ratchet_tree,
            group_info: pq.group_info,
        });
        CreateGroupParamsOut {
            group_id: self.group_id,
            ratchet_tree: self.ratchet_tree,
            encrypted_user_profile_key,
            creator_client_reference,
            group_info: self.group_info,
            room_state: self.room_state,
            pq,
            creator_user_credential: None,
            group_bootstrap: None,
        }
    }
}

#[derive(Debug)]
pub(super) struct DecryptedProfileInfos {
    /// Profile infos of *other* members
    pub(super) members: Vec<ProfileInfo>,
    /// None = DS entry missing or undecryptable
    pub(super) own_profile_key: Option<UserProfileKey>,
}

#[derive(Debug)]
pub(super) struct ProfileInfo {
    pub(super) user_credential: UserCredential,
    pub(super) user_profile_key: UserProfileKey,
}

impl From<(UserCredential, UserProfileKey)> for ProfileInfo {
    fn from((user_credential, user_profile_key): (UserCredential, UserProfileKey)) -> Self {
        Self {
            user_credential,
            user_profile_key,
        }
    }
}

/// Candidate signing keys for the DS `welcome_info` lookups when joining a
/// group. The right key is the one matching the joiner's leaf credential.
pub(super) struct JoinSigners<'a> {
    /// The shared user signing key. Key packages derived from a sibling's
    /// upload always carry the shared user credential.
    pub(super) client: &'a UserSigningKey,
    /// The self-group signing key, if provisioned. A freshly linked device
    /// joins the self-group with it.
    pub(super) self_group: Option<&'a SelfGroupSigningKey>,
}

impl JoinSigners<'_> {
    /// The signing key matching the given joiner leaf signature key, if any.
    fn for_joiner_leaf(
        &self,
        signature_key: &SignaturePublicKey,
    ) -> Option<&SigningKey<ClientKeyType>> {
        let candidates: [Option<&SigningKey<ClientKeyType>>; 2] =
            [Some(self.client), self.self_group.map(|signer| &**signer)];
        candidates.into_iter().flatten().find(|candidate| {
            &SignaturePublicKey::from(candidate.verifying_key().clone()) == signature_key
        })
    }
}

/// One contact that has been prepared for inviting to a group.
pub(crate) struct PreparedInvitee {
    pub(crate) add_info: ContactAddInfos,
    pub(crate) wai_key: WelcomeAttributionInfoEarKey,
    pub(crate) user_credential: UserCredential,
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
struct SendMessageCollisionKey {
    // The group epoch this secret key was exported from.
    epoch: GroupEpoch,
    // The HKDF instance used to derive the collision tag.
    kdf: Hkdf<Sha256>,
}

impl SendMessageCollisionKey {
    pub fn try_from_group(
        group: &mut Group,
        provider: &AirOpenMlsProvider,
    ) -> Result<Self, ExportSecretError> {
        let salt = group.own_index().u32().to_le_bytes();
        let epoch = group.mls_group.epoch();
        let epoch_secret = group.mls_group.export_secret(
            provider.crypto(),
            "virtual-client-collision-detection-v1",
            &salt,
            32,
        )?;

        let kdf = Hkdf::from_prk(&epoch_secret).expect("input is 32 bytes, a valid HKDF PRK");
        Ok(Self { epoch, kdf })
    }

    fn derive_collision_tag(&self, prefix: &'static str, info: &[u8]) -> CollisionTag {
        // our KDF is using SHA-256 but you can request less bytes, it is just truncated internally.
        let mut tag: [u8; 8] = [0u8; 8];
        let info = &[prefix.as_bytes(), info].concat();
        self.kdf
            .expand(info.as_slice(), &mut tag)
            .expect("8 bytes is a valid HKDF-Expand SHA-256 truncated output length");
        CollisionTag::new(i64::from_le_bytes(tag))
    }
}

trait GenerateCollisionTag {
    fn collision_tag(&self, key: &SendMessageCollisionKey) -> SendMessageCollisionTag;
}

fn generation_id_to_collision_tag(generation_id: &GenerationId) -> CollisionTag {
    let mut buf = [0u8; 8];
    let slice = generation_id.as_slice();
    let n = slice.len().min(8);
    buf[..n].copy_from_slice(&slice[..n]);
    CollisionTag::new(i64::from_le_bytes(buf))
}

impl GenerateCollisionTag for PerMessageStatus {
    fn collision_tag(&self, key: &SendMessageCollisionKey) -> SendMessageCollisionTag {
        match self.status {
            MessageStatus::Delivered => SendMessageCollisionTag::DeliveryReceipt(
                key.derive_collision_tag("delivered", &self.mimi_id),
            ),
            MessageStatus::Read => SendMessageCollisionTag::ReadReceipt(
                key.derive_collision_tag("read", &self.mimi_id),
            ),
            _ => SendMessageCollisionTag::Other(key.derive_collision_tag("aux", &self.mimi_id)),
        }
    }
}

/// A chat group as seen by this client.
///
/// `Group` bundles MLS state (`mls_group` and, for APQ groups, `pq`) with the
/// Air-level secrets and policy state that ride alongside MLS.
///
#[derive(Debug)]
pub(crate) struct Group {
    identity_link_wrapper_key: IdentityLinkWrapperKey,
    group_state_ear_key: GroupStateEarKey,
    mls_group: MlsGroup,
    /// `room_state` is the Air/MIMI room policy state. It must be advanced
    /// together with MLS proposal/commit application (see
    /// [`Group::apply_staged_operations_to_room_state`] and
    /// [`Group::merge_pending_commit`]).
    room_state: VerifiedRoomState,
    pending_diff: Option<StagedGroupDiff>, // Currently unused, but we're keeping it for later
    /// The time at which the user self-updated their key material in this group the last time
    pub(crate) self_updated_at: Option<TimeStamp>,
    pq: Option<PqGroup>,
    /// Set when a commit send fails non-transiently. Cleared on merge or discard.
    pending_commit_failed: bool,
    /// Symmetric key used as the PRK for collision-detection tag derivation.
    ///
    /// Set by the application on every group epoch change.
    send_message_collision_key: Option<SendMessageCollisionKey>,
    /// The user id of this client. Used to resolve the owner of self-group leaves, which carry no
    /// user identity of their own.
    own_user_id: UserId,
}

impl Group {
    pub(crate) fn is_apq(&self) -> bool {
        self.pq.is_some()
    }

    pub(crate) fn mls_group(&self) -> &MlsGroup {
        &self.mls_group
    }

    pub(crate) fn mls_group_mut(&mut self) -> &mut MlsGroup {
        &mut self.mls_group
    }

    pub(crate) fn pq(&self) -> Option<&PqGroup> {
        self.pq.as_ref()
    }

    /// Returns the PQ group ID of this group, if it is an APQ group.
    ///
    /// The ID is read from the APQMLS component in the T group's context extensions, so it is
    /// available even when the local PQ group state is missing (e.g. after a legacy T-only resync).
    pub(crate) fn pq_group_id(&self) -> Option<GroupId> {
        let info = ApqInfo::from_extensions(self.mls_group.extensions())
            .inspect_err(|error| error!(%error, "Failed to parse APQMLS component"))
            .ok()??;
        Some(info.pq_session_group_id)
    }

    pub(crate) fn pq_mut(&mut self) -> Option<&mut PqGroup> {
        self.pq.as_mut()
    }

    /// Whether the group has a commit that the DS rejected and that has not
    /// been reconciled yet.
    #[cfg(test)]
    pub(crate) fn commit_failed(&self) -> bool {
        self.pending_commit_failed
    }

    pub(crate) async fn mark_commit_failed(
        &mut self,
        mut connection: impl WriteConnection,
    ) -> sqlx::Result<()> {
        error!(group_id = ?self.group_id(), "Group is desynced");
        if !self.pending_commit_failed {
            self.pending_commit_failed = true;
            self.store_pending_commit_failed(&mut connection).await?;

            if let Some(chat_id) =
                ChatId::load_from_group_id(&mut connection, self.group_id()).await?
            {
                connection.notifier().update(chat_id);
            }
        }

        Ok(())
    }

    pub(crate) async fn clear_commit_failed(
        &mut self,
        mut connection: impl WriteConnection,
    ) -> sqlx::Result<()> {
        if self.pending_commit_failed {
            self.pending_commit_failed = false;
            self.store_pending_commit_failed(&mut connection).await?;

            if let Some(chat_id) =
                ChatId::load_from_group_id(&mut connection, self.group_id()).await?
            {
                connection.notifier().update(chat_id);
            }
        }
        Ok(())
    }

    /// Returns mutable references to the T MLS group and the PQ MLS group
    /// together, for callers that need to pass both into
    /// [`apqmls::commit_builder::CommitBuilder::from_groups`]. Errors for
    /// non-APQ groups.
    pub(crate) fn apq_mls_groups_mut(&mut self) -> Result<(&mut MlsGroup, &mut MlsGroup)> {
        let Self { mls_group, pq, .. } = self;
        let pq = pq.as_mut().context("No PQ group found")?;
        Ok((mls_group, &mut pq.mls_group))
    }

    /// Consumes this group and returns its room state. Used by callers
    /// that no longer need the rest of the group.
    pub(crate) fn into_room_state(self) -> VerifiedRoomState {
        self.room_state
    }

    /// Errors if this group (or its PQ counterpart, for APQ groups) has a
    /// pending commit. Used by clean loaders to refuse to hand out a
    /// `Group` whose MLS state has an in-flight commit, since further
    /// staging on top of one is a logic error.
    pub(crate) fn ensure_clean(&self) -> Result<()> {
        ensure!(
            self.mls_group.pending_commit().is_none(),
            "Room already had a pending commit"
        );
        if let Some(pq) = self.pq.as_ref() {
            ensure!(
                pq.mls_group.pending_commit().is_none(),
                "PQ Room already had a pending commit"
            );
        }
        Ok(())
    }

    /// Returns the [`AirComponent`] from the leaf node of the given member, or `None` if the member
    /// is not in the group.
    pub(crate) fn member_air_component(&self, user_id: &UserId) -> Option<AirComponent> {
        let member = self.mls_group.members().find(|m| {
            LeafCredential::from_credential(&m.credential)
                .map(|c| c.user_id(self.own_user_id()) == user_id)
                .unwrap_or(false)
        })?;

        let leaf_node = self.mls_group.public_group().leaf(member.index)?;
        leaf_node
            .extensions()
            .app_data_dictionary()
            .and_then(|dict| dict.dictionary().get(&AIR_COMPONENT_ID))
            .and_then(|data| {
                AirComponent::from_bytes(data)
                    .inspect_err(|error| {
                        error!(%error, "Failed to deserialize member air component");
                    })
                    .ok()
            })
    }

    pub(crate) fn members_air_component(&self) -> impl Iterator<Item = Option<AirComponent>> {
        self.mls_group.members().map(|member| {
            let leaf_node = self.mls_group.public_group().leaf(member.index)?;
            let dict = leaf_node.extensions().app_data_dictionary()?;
            let data = dict.dictionary().get(&AIR_COMPONENT_ID)?;
            AirComponent::from_bytes(data)
                .inspect_err(|error| {
                    error!(%error, "Failed to deserialize member air component");
                })
                .ok()
        })
    }

    /// Create a group.
    pub(super) fn create_group(
        mut connection: impl WriteConnection,
        signer: &UserSigningKey,
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
            .with_capabilities(default_leaf_node_capabilities())
            .with_group_context_extensions(gc_extensions)
            .with_leaf_node_extensions(default_leaf_node_extensions::<AirComponent>())?
            .sender_ratchet_configuration(default_sender_ratchet_configuration())
            .max_past_epochs(MAX_PAST_EPOCHS)
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .build(&provider, signer, credential_with_key)
            .map_err(|e| anyhow!("Error while creating group: {:?}", e))?;

        let creator_identity = RoomPolicyIdentity::User(signer.credential().user_id().clone());
        let room_state = VerifiedRoomState::new(
            creator_identity.to_bytes()?,
            RoomPolicy::default_trusted_private(),
        )?;

        let params = PartialCreateGroupParams {
            group_id: group_id.clone(),
            ratchet_tree: mls_group.export_ratchet_tree(),
            group_info: mls_group.export_group_info(provider.crypto(), signer, true)?,
            room_state: room_state.clone(),
            pq: None,
        };

        let group = Self {
            identity_link_wrapper_key,
            mls_group,
            room_state,
            group_state_ear_key: group_state_ear_key.clone(),
            pending_diff: None,
            self_updated_at: Some(TimeStamp::now()),
            pq: None,
            pending_commit_failed: false,
            send_message_collision_key: None,
            own_user_id: signer.credential().user_id().clone(),
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
        signer: &UserSigningKey,
    ) -> Result<(Self, UserId, DecryptedProfileInfos)> {
        let serialized_welcome = welcome_bundle.welcome.tls_serialize_detached()?;

        let mls_group_config = default_mls_group_join_config();

        let (processed_welcome, joiner_info) = {
            // Phase 1: Resolve our key material for the welcome
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            let key_material = welcome_bundle
                .welcome
                .welcome
                .resolve_own_key_material(&provider)?
                .ok_or(GroupOperationError::MissingKeyPackage)?;

            // Phase 2: Process the welcome message
            let info = &[];
            let aad = &[];
            let decryption_key = JoinerInfoDecryptionKey::from((
                key_material.init_private_key().clone(),
                key_material.hpke_init_key().clone(),
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
            indexed_encrypted_user_profile_keys,
        } = welcome_info;

        let (mls_group, joiner_info, verifiable_attribution_info) = {
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

            (mls_group, joiner_info, verifiable_attribution_info)
        };

        // Self-groups are only ever joined during device linking, which uses the APQ join path.
        ensure!(
            !AirComponent::is_self_group_context(mls_group.extensions()),
            "refusing to join a group marked as self-group"
        );

        let credentials =
            verify_member_credentials(&mut *txn, api_clients, &mls_group, false).await?;
        ensure_room_state_users_are_members(&room_state, &mls_group)?;

        let sender_user_id = verifiable_attribution_info.sender();
        let sender_user_credential =
            match StorableUserCredential::load_by_user_id(&mut *txn, &sender_user_id).await? {
                Some(credential) => credential,
                // A linked device may not know the inviter yet => the inviter is a member, so use
                // it AS-verified member credentials
                None => credentials
                    .iter()
                    .find(|c| c.user_id() == &sender_user_id)
                    .cloned()
                    .context("sender is not a member of the group")?,
            };

        if BlockedContact::check_blocked(&mut *txn, &sender_user_id).await? {
            bail!(BlockedContactError);
        }

        let welcome_attribution_info: WelcomeAttributionInfoPayload =
            verifiable_attribution_info.verify(sender_user_credential.verifying_key())?;

        let group = Self {
            mls_group,
            identity_link_wrapper_key: welcome_attribution_info.identity_link_wrapper_key().clone(),
            group_state_ear_key: joiner_info.group_state_ear_key,
            pending_diff: None,
            room_state,
            self_updated_at: Some(TimeStamp::now()),
            pq: None,
            pending_commit_failed: false,
            send_message_collision_key: None,
            own_user_id: signer.credential().user_id().clone(),
        };

        // Phase 7: Store the group and user credentials.
        group.store(&mut *txn).await?;
        for credential in &credentials {
            credential.store(&mut *txn).await?;
        }

        // Phase 8: Decrypt profile keys
        let encrypted_user_profile_keys_fallback = if indexed_encrypted_user_profile_keys.is_empty()
        {
            credentials
                .iter()
                .map(|c| c.user_id().clone())
                .zip(encrypted_user_profile_keys)
                .collect()
        } else {
            Default::default()
        };
        let member_profile_info = group.decrypt_member_profile_keys(
            credentials,
            indexed_encrypted_user_profile_keys,
            encrypted_user_profile_keys_fallback,
        );

        Ok((group, sender_user_id, member_profile_info))
    }

    /// Same as [`Self::join_group`], but for APQ groups.
    pub(super) async fn join_apq_group(
        welcome_bundle: ApqWelcomeBundle,
        // This is our own key that the sender uses to encrypt to us. We should
        // be able to retrieve it from the client's key store.
        welcome_attribution_info_ear_key: &WelcomeAttributionInfoEarKey,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        signers: JoinSigners<'_>,
    ) -> Result<(Self, UserId, DecryptedProfileInfos)> {
        // Phase 1: Serialize welcome and split
        let serialized_welcome = welcome_bundle.welcome.tls_serialize_detached()?;
        let (t_welcome, pq_welcome) = welcome_bundle.welcome.split();
        let mls_group_config = default_mls_group_join_config();
        let t_ciphersuite = t_welcome.ciphersuite();

        // Phase 2: Resolve T key material, decrypt joiner info, process PQ welcome
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let t_key_material = t_welcome
            .resolve_own_key_material(&provider)?
            .ok_or(GroupOperationError::MissingKeyPackage)?;

        // The DS keys `welcome_info` on the joiner's leaf signature key, so we
        // must sign those requests with the matching signing key. A key
        // package derived from a sibling's upload has no local KeyPackage and
        // always carries the shared client credential.
        let signer: &SigningKey<ClientKeyType> = match t_key_material.key_package_bundle() {
            Some(bundle) => signers
                .for_joiner_leaf(bundle.key_package().leaf_node().signature_key())
                .context("no candidate signing key matches the joiner leaf")?,
            None => signers.client,
        };

        // DS joiner info is encrypted with the T-key package private key
        let info = &[];
        let aad = &[];
        let decryption_key = JoinerInfoDecryptionKey::from((
            t_key_material.init_private_key().clone(),
            t_key_material.hpke_init_key().clone(),
        ));
        let joiner_info = DsJoinerInformation::decrypt(
            welcome_bundle.encrypted_joiner_info,
            &decryption_key,
            info,
            aad,
        )?;

        let processed_pq_welcome =
            ProcessedWelcome::new_from_welcome(&provider, &mls_group_config, pq_welcome)?;

        let pq_group_id = processed_pq_welcome.unverified_group_info().group_id();
        let pq_qgid = QualifiedGroupId::try_from(pq_group_id)?;
        let api_client = api_clients.get(pq_qgid.owning_domain())?;
        let WelcomeInfoIn {
            ratchet_tree: pq_ratchet_tree,
            encrypted_user_profile_keys: _,
            room_state: _,
            indexed_encrypted_user_profile_keys: _,
        } = api_client
            .ds_welcome_info(
                pq_group_id.clone(),
                processed_pq_welcome.unverified_group_info().epoch(),
                &joiner_info.group_state_ear_key,
                signer,
            )
            .await?;

        // Check if there is already a group with the same ID.
        if let Some(t_group_id) = Self::load_group_id_for_pq(&mut *txn, pq_group_id).await? {
            // If the group is active, we can't join it.
            if Self::is_active(&mut *txn, &t_group_id)? {
                bail!("We can't join a group that is still active.");
            }
            // Otherwise, we delete the old group.
            Self::delete_from_db(txn, &t_group_id).await?;
        }

        // Phase 3: Complete the PQ join first, then derive the PSK needed by the T welcome.
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let pq_builder = JoinBuilder::new(&provider, processed_pq_welcome)
            .skip_lifetime_validation()
            .with_ratchet_tree(pq_ratchet_tree);
        let mut pq_mls_group = pq_builder.build()?.into_group(&provider)?;

        // Note: This method has a side-effect of storing PSK in the database. It is important to
        // call it *after* processing the PQ welcome and *before* processing the T welcome.
        let apq_psk_id = apqmls::welcome::derive_and_store_join_psk(
            &provider,
            &mut pq_mls_group,
            t_ciphersuite,
        )?;

        let processed_t_welcome =
            ProcessedWelcome::new_from_welcome(&provider, &mls_group_config, t_welcome)?;

        // The T welcome must import the PSK exported from the PQ session, or the
        // T session we are about to join carries no PQ contribution.
        validate_welcome_psk(&processed_t_welcome, &apq_psk_id)
            .context("T welcome does not import the APQ PSK")?;

        // Check if there is already a group with the same ID.
        let t_group_id = processed_t_welcome.unverified_group_info().group_id();
        let t_qgid = QualifiedGroupId::try_from(t_group_id)?;
        ensure!(
            t_qgid.owning_domain() == pq_qgid.owning_domain(),
            "T and PQ groups must belong to the same domain"
        );
        if let Some(group) = Self::load(&mut *txn, t_group_id).await? {
            if group.mls_group().is_active() {
                bail!("Joining new group which is still active");
            }
            Self::delete_from_db(txn, t_group_id).await?;
        }

        // Phase 4: Fetch the T welcome info and complete the T join.
        let WelcomeInfoIn {
            ratchet_tree: t_ratchet_tree,
            encrypted_user_profile_keys,
            room_state,
            indexed_encrypted_user_profile_keys,
        } = api_client
            .ds_welcome_info(
                t_group_id.clone(),
                processed_t_welcome.unverified_group_info().epoch(),
                &joiner_info.group_state_ear_key,
                signer,
            )
            .await?;

        let is_self_group = OwnClientInfo::is_own_self_group(&mut *txn, t_group_id).await?;

        let t_mls_group = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            let t_builder = JoinBuilder::new(&provider, processed_t_welcome)
                .skip_lifetime_validation()
                .with_ratchet_tree(t_ratchet_tree);
            // The self group is the emulation group, so joining it registers the
            // derivation epoch we join into.
            t_builder
                .build()?
                .emulation_group(is_self_group)
                .into_group(&provider)?
        };

        // The PQ leaf deliberately carries an empty credential, so membership is
        // compared structurally rather than by credential equality. The signing
        // key is what binds a member's two leaves together for us.
        validate_apq_session_at_construction(&t_mls_group, &pq_mls_group, |_, _| true)
            .context("invalid APQ session")?;
        verify_pq_signature_keys(&t_mls_group, &pq_mls_group)
            .context("T and PQ membership is not bound by matching signature keys")?;

        // Phase 5: Verify WAI + extract sender
        let verifiable_attribution_info = WelcomeAttributionInfo::decrypt(
            welcome_attribution_info_ear_key,
            &welcome_bundle.encrypted_attribution_info,
        )?
        .into_verifiable(t_mls_group.group_id().clone(), serialized_welcome);

        let sender_user_id = verifiable_attribution_info.sender();
        if BlockedContact::check_blocked(&mut *txn, &sender_user_id).await? {
            bail!(BlockedContactError);
        }

        // Phase 6: Construct and persist Group.
        //
        // A group flagged as self-group may only be joined during device linking, i.e. when it is
        // recorded as our own self-group. Conversely, our own self-group must carry the flag,
        // since its self-group credentials are only accepted there.
        ensure!(
            AirComponent::is_self_group_context(t_mls_group.extensions()) == is_self_group
                && AirComponent::is_self_group_context(pq_mls_group.extensions()) == is_self_group,
            "self-group flag does not match the recorded self-group"
        );
        let credentials =
            verify_member_credentials(txn, api_clients, &t_mls_group, is_self_group).await?;
        ensure_room_state_users_are_members(&room_state, &t_mls_group)?;

        let sender_user_credential =
            match StorableUserCredential::load_by_user_id(&mut *txn, &sender_user_id).await? {
                Some(credential) => credential,
                // A linked device may not know the inviter yet => the inviter is a member, so use
                // it AS-verified member credentials
                None => credentials
                    .iter()
                    .find(|c| c.user_id() == &sender_user_id)
                    .cloned()
                    .context("sender is not a member of the group")?,
            };
        let welcome_attribution_info: WelcomeAttributionInfoPayload =
            verifiable_attribution_info.verify(sender_user_credential.verifying_key())?;

        let self_updated_at = TimeStamp::now();
        let group = Self {
            identity_link_wrapper_key: welcome_attribution_info.identity_link_wrapper_key().clone(),
            group_state_ear_key: joiner_info.group_state_ear_key,
            mls_group: t_mls_group,
            room_state,
            pending_diff: None,
            self_updated_at: Some(self_updated_at),
            pq: Some(PqGroup {
                mls_group: pq_mls_group,
                self_updated_at: Some(self_updated_at),
            }),
            pending_commit_failed: false,
            send_message_collision_key: None,
            own_user_id: signers.client.credential().user_id().clone(),
        };
        group.store(&mut *txn).await?;
        for credential in &credentials {
            credential.store(&mut *txn).await?;
        }

        // Phase 7: Decrypt profile keys
        let encrypted_user_profiles_keys_fallback =
            if indexed_encrypted_user_profile_keys.is_empty() {
                credentials
                    .iter()
                    .map(|c| c.user_id().clone())
                    .zip(encrypted_user_profile_keys)
                    .collect()
            } else {
                Default::default()
            };
        let member_profile_info = group.decrypt_member_profile_keys(
            credentials,
            indexed_encrypted_user_profile_keys,
            encrypted_user_profiles_keys_fallback,
        );

        Ok((group, sender_user_id, member_profile_info))
    }

    /// Pair the encrypted user profile keys with the group members and decrypt them.
    ///
    /// Keys that are missing or fail to decrypt are skipped with a warning: a stale DS entry must
    /// not fail the join/resync.
    fn decrypt_member_profile_keys(
        &self,
        credentials: Vec<StorableUserCredential>,
        indexed_keys: HashMap<LeafNodeIndex, EncryptedUserProfileKey>,
        // Positional fallback for servers that don't send indexed keys yet
        fallback_keys: HashMap<UserId, EncryptedUserProfileKey>,
    ) -> DecryptedProfileInfos {
        let own_user_id = self.own_user_id();
        let indices = self.mls_group().members().map(|m| m.index);

        let mut members = Vec::with_capacity(credentials.len());
        let mut own_profile_key = None;

        for (index, credential) in indices.zip(credentials) {
            let eupk = if !indexed_keys.is_empty() {
                indexed_keys.get(&index)
            } else {
                fallback_keys.get(credential.user_id())
            };
            let Some(eupk) = eupk else {
                if credential.user_id() != own_user_id {
                    // Own key is sometimes expected to be missing
                    warn!(
                        user_id =? credential.user_id(),
                        "No user profile key for member; skipping"
                    );
                }
                continue;
            };
            match UserProfileKey::decrypt(
                &self.identity_link_wrapper_key,
                eupk,
                credential.user_id(),
            ) {
                Ok(user_profile_key) => {
                    if credential.user_id() == own_user_id {
                        own_profile_key = Some(user_profile_key);
                    } else {
                        members.push(ProfileInfo {
                            user_profile_key,
                            user_credential: credential.into(),
                        });
                    }
                }
                Err(error) => {
                    warn!(
                        %error,
                        user_id =? credential.user_id(),
                        "Failed to decrypt user profile key; skipping"
                    );
                    continue;
                }
            }
        }
        DecryptedProfileInfos {
            members,
            own_profile_key,
        }
    }

    /// Build the positional fallback map (user id -> encrypted profile key).
    ///
    /// Used when the commit info carries no indexed keys. Credentials are verified later, so
    /// undecodable leaves are simply skipped here.
    fn encrypted_profile_keys_fallback(
        ratchet_tree: &RatchetTreeIn,
        encrypted_user_profile_keys: Vec<EncryptedUserProfileKey>,
        indexed_encrypted_user_profile_keys: &HashMap<LeafNodeIndex, EncryptedUserProfileKey>,
        own_user_id: &UserId,
    ) -> HashMap<UserId, EncryptedUserProfileKey> {
        if !indexed_encrypted_user_profile_keys.is_empty() {
            return HashMap::new();
        }
        ratchet_tree
            .leaves()
            .zip(encrypted_user_profile_keys)
            .filter_map(|(leaf_node, profile_key)| {
                let cred = LeafCredential::from_credential(leaf_node.credential()).ok()?;
                Some((cred.user_id(own_user_id).clone(), profile_key))
            })
            .collect()
    }

    /// Persist a freshly joined group after an external commit.
    ///
    /// Replace any prior group with the same id, store the user credentials, and decrypt the
    /// member profile keys.
    async fn store_after_external_join(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        credentials: Vec<StorableUserCredential>,
        indexed_encrypted_user_profile_keys: HashMap<LeafNodeIndex, EncryptedUserProfileKey>,
        encrypted_profile_keys_fallback: HashMap<UserId, EncryptedUserProfileKey>,
    ) -> anyhow::Result<DecryptedProfileInfos> {
        // If the group previously existed, delete it first.
        Group::delete_from_db(txn, self.group_id()).await?;
        self.store(&mut *txn).await?;

        for credential in &credentials {
            credential.store(&mut *txn).await?;
        }

        Ok(self.decrypt_member_profile_keys(
            credentials,
            indexed_encrypted_user_profile_keys,
            encrypted_profile_keys_fallback,
        ))
    }

    /// Join a group using an external commit.
    #[expect(clippy::too_many_arguments)]
    pub(super) async fn join_group_externally(
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        external_commit_info: ExternalCommitInfoIn,
        signer: &UserSigningKey,
        group_state_ear_key: GroupStateEarKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        aad: AadMessage,
        // Should be Some if this join is in response to a connection offer.
        connection_offer_hash: Option<ConnectionOfferHash>,
        // Should be Some if we are joining as an emulator of a virtual client
        // that is already a member.
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
            room_state,
            proposals,
            pq,
        } = external_commit_info;

        ensure!(pq.is_none(), "APQ group in non-APQ stage_invite");

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

            let leaf_node_extensions = if vc_group_id.is_some() {
                vc_leaf_node_extensions::<AirComponent>()
            } else {
                default_leaf_node_extensions::<AirComponent>()
            };
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

        // A group flagged as self-group must be recorded as our own self-group and vice versa.
        let is_self_group =
            OwnClientInfo::is_own_self_group(&mut *txn, mls_group.group_id()).await?;
        ensure!(
            AirComponent::is_self_group_context(mls_group.extensions()) == is_self_group,
            "self-group flag does not match the recorded self-group"
        );

        // Phase 3: Verify the user credentials. Self-group leaves carry a self-group credential,
        // which is only accepted inside our own self group.
        let credentials =
            verify_member_credentials(&mut *txn, api_clients, &mls_group, is_self_group).await?;
        ensure_room_state_users_are_members(&room_state, &mls_group)?;

        let group = Self {
            mls_group,
            identity_link_wrapper_key,
            group_state_ear_key,
            pending_diff: None,
            room_state,
            self_updated_at: Some(TimeStamp::now()),
            pq: None,
            pending_commit_failed: false,
            send_message_collision_key: None,
            own_user_id: signer.credential().user_id().clone(),
        };

        // Phase 4: Store the group and client auth info.
        let member_profile_info = group
            .store_after_external_join(
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
    pub(super) async fn join_apq_group_externally(
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        external_commit_info: ExternalCommitInfoIn,
        signer: &LeafSigningKey,
        own_user_id: &UserId,
        group_state_ear_key: GroupStateEarKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        aad: AadMessage,
        vc_group_id: Option<GroupId>,
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
            room_state,
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

        let group_info = VerifiableApqGroupInfo::new(t_group_info, pq_group_info);

        let encrypted_profile_keys_fallback = Self::encrypted_profile_keys_fallback(
            &t_ratchet_tree,
            encrypted_user_profile_keys,
            &indexed_encrypted_user_profile_keys,
            own_user_id,
        );

        let ratchet_tree = ApqRatchetTreeIn::new(t_ratchet_tree, pq_ratchet_tree);

        // Build the group
        let mls_group_config = default_mls_group_join_config();
        let leaf_node_extensions = if vc_group_id.is_some() {
            vc_leaf_node_extensions::<AirComponent>()
        } else {
            default_leaf_node_extensions::<AirComponent>()
        };
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

        // A group flagged as self-group must be recorded as our own self-group and vice versa.
        let is_self_group = OwnClientInfo::is_own_self_group(&mut *txn, t_group.group_id()).await?;
        ensure!(
            AirComponent::is_self_group_context(t_group.extensions()) == is_self_group
                && AirComponent::is_self_group_context(pq_group.extensions()) == is_self_group,
            "self-group flag does not match the recorded self-group"
        );
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
        ensure_room_state_users_are_members(&room_state, &t_group)?;

        // Store the group, credentials and member profile infos
        let now = TimeStamp::now();
        let group = Self {
            mls_group: t_group,
            identity_link_wrapper_key,
            group_state_ear_key,
            pending_diff: None,
            room_state,
            self_updated_at: Some(now),
            pq: Some(PqGroup {
                mls_group: pq_group,
                self_updated_at: Some(now),
            }),
            pending_commit_failed: false,
            send_message_collision_key: None,
            own_user_id: own_user_id.clone(),
        };

        let member_profile_info = group
            .store_after_external_join(
                txn,
                credentials,
                indexed_encrypted_user_profile_keys,
                encrypted_profile_keys_fallback,
            )
            .await?;

        Ok(Ok((group, commit_bundle, member_profile_info)))
    }

    /// Invite the given list of contacts to join the group.
    ///
    /// Returns the [`GroupOperationParamsOut`] as input for the pending chat operation processing.
    pub(super) async fn stage_invite(
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

    /// Validate the leaf credential of a client about to be added to this self-group: it must be
    /// a self-group credential whose client id is not yet used by any existing member.
    pub(crate) fn validate_self_group_add(&self, added: &Credential) -> Result<()> {
        validate_self_group_add_credential(
            self.mls_group.members().map(|member| member.credential),
            added,
        )
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
    pub(super) async fn stage_apq_invite(
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

    pub(super) async fn stage_remove(
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

    pub(super) async fn stage_apq_remove(
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

    pub(super) async fn stage_delete(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &UserSigningKey,
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

        // There shouldn't be a welcome
        let aad_payload = AadPayload::DeleteGroup;
        let aad = AadMessage::from(aad_payload).tls_serialize_detached()?;
        self.mls_group.set_aad(aad);

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

        debug_assert!(_welcome_option.is_none());
        let group_info =
            group_info_option.ok_or(anyhow!("No group info after commit operation"))?;
        let commit = AssistedMessageOut::new(mls_message, Some(group_info.into()));

        let params = DeleteGroupParamsOut { commit };
        Ok(params)
    }

    pub(super) async fn stage_apq_delete(
        &mut self,
        mut connection: impl WriteConnection,
        signer: &UserSigningKey,
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

        let (t_group, pq_group) = self.apq_mls_groups_mut()?;

        let aad_payload = AadPayload::DeleteGroup;
        let aad = AadMessage::from(aad_payload).tls_serialize_detached()?;
        t_group.set_aad(aad);

        let mut builder = apqmls::commit_builder::CommitBuilder::from_groups(t_group, pq_group)
            .force_self_update(true)
            .propose_removals(removed_indices)
            .create_group_info(true);
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

    pub(super) async fn discard_pending_commit(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
    ) -> Result<()> {
        self.clear_commit_failed(&mut *txn).await?;
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        self.pending_diff = None;
        // Clear the pending commit in the T group...
        self.mls_group.clear_pending_commit(provider.storage())?;
        // ... and in the PQ group if it exists.
        if let Some(pq) = &mut self.pq {
            pq.mls_group.clear_pending_commit(provider.storage())?;
        }
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
            self.room_state_change_role_identity(&remover, &removed, RoleIndex::Outsider)?;
        }
        for (adder, added) in self.pending_adds(staged_commit) {
            self.room_state_change_role_identity(&adder, &added, RoleIndex::Regular)?;
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
        let provider = AirOpenMlsProvider::new(txn.as_mut());

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

            self.mls_group
                .merge_staged_commit(&provider, staged_commit)?;
            if let Some(pq) = &mut self.pq
                && pq.mls_group.pending_commit().is_some()
            {
                pq.mls_group.merge_pending_commit(&provider)?;
            }
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

            self.mls_group.merge_pending_commit(&provider)?;
            if let Some(pq) = &mut self.pq
                && pq.mls_group.pending_commit().is_some()
            {
                pq.mls_group.merge_pending_commit(&provider)?;
            }
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
        self.send_message_collision_key = None;
        self.clear_commit_failed(&mut *txn).await?;

        // The linked-device list joins metadata with the live self-group
        // members. Notify its self-chat listener whenever a commit changes the
        // locally stored self group.
        if self.is_self_group()
            && let Some(chat_id) = ChatId::load_from_group_id(&mut *txn, self.group_id()).await?
        {
            txn.notifier().update(chat_id);
        }

        Ok((event_messages, group_data))
    }

    /// Derive and register the collision-detection key for the current epoch if not already set
    pub(crate) fn ensure_collision_key(
        &mut self,
        provider: &AirOpenMlsProvider,
    ) -> Result<(), ExportSecretError> {
        if let Some(key) = self.send_message_collision_key.as_ref()
            && key.epoch == self.mls_group.epoch()
        {
            return Ok(());
        }

        self.send_message_collision_key =
            Some(SendMessageCollisionKey::try_from_group(self, provider)?);
        Ok(())
    }

    /// Send an application message to the group.
    ///
    /// Collision tags are only included when a collision key has been registered via
    /// `ensure_collision_key` (i.e. the group is acting as a virtual client).
    pub(super) fn create_message(
        &mut self,
        provider: &AirOpenMlsProvider<'_>,
        signer: &impl Signer,
        content: MimiContent,
        message_status_report: Option<MessageStatusReport>,
    ) -> Result<SendMessageParamsOut, GroupOperationError> {
        let UnconfirmedMessage {
            message,
            epoch,
            generation,
            generation_id,
        } = self
            .mls_group
            .create_unconfirmed_message(provider, signer, &content.serialize()?)?;

        let mut collision_tags = Vec::new();
        if let Some(generation_id) = generation_id {
            collision_tags.push(SendMessageCollisionTag::Generation(
                generation_id_to_collision_tag(&generation_id),
            ));

            if let Some(message_status_report) = message_status_report {
                self.ensure_collision_key(provider)?;
                if let Some(key) = self.send_message_collision_key.as_ref() {
                    for pms in message_status_report.statuses {
                        collision_tags.push(pms.collision_tag(key));
                    }
                }
            }
        }

        let message = AssistedMessageOut::new(message, None);
        let suppress_notifications = suppress_notifications(&content);

        let send_message_params = SendMessageParamsOut {
            sender: self.mls_group.own_leaf_index(),
            message,
            suppress_notifications,
            epoch,
            generation,
            collision_tags,
        };

        Ok(send_message_params)
    }

    /// Send an application message to the group.
    pub(super) fn create_targeted_application_message(
        &mut self,
        provider: &AirOpenMlsProvider<'_>,
        signer: &UserSigningKey,
        recipient: UserId,
        content: TargetedMessageContent,
    ) -> Result<TargetedMessageParamsOut, GroupOperationError> {
        let content_bytes = content.tls_serialize_detached()?;
        let UnconfirmedMessage {
            message,
            generation,
            generation_id,
            epoch: _,
        } = self
            .mls_group
            .create_unconfirmed_message(provider, signer, &content_bytes)?;

        let mut collision_tags = Vec::new();
        if let Some(generation_id) = generation_id {
            collision_tags.push(SendMessageCollisionTag::Generation(
                generation_id_to_collision_tag(&generation_id),
            ));
        }

        let message = AssistedMessageOut::new(message, None);

        let recipient_index = self
            .mls_group()
            .members()
            .find_map(|m| {
                let user_credential = LeafCredential::from_credential(&m.credential).ok()?;
                if user_credential.user_id(self.own_user_id()) == &recipient {
                    Some(m.index)
                } else {
                    None
                }
            })
            .ok_or(TargetedMessageError::RecipientNotInGroup)?;

        let params = TargetedMessageParamsOut {
            sender: self.mls_group.own_leaf_index(),
            generation,
            collision_tags,
            message_type: TargetedMessageType::ApplicationMessage {
                message,
                recipient: recipient_index,
            },
        };

        Ok(params)
    }

    /// Mark the message sent at this generation as confirmed (accepted by DS).
    pub(crate) fn confirm_application_message(
        &mut self,
        provider: &AirOpenMlsProvider<'_>,
        epoch: GroupEpoch,
        generation: u32,
    ) -> Result<(), GroupOperationError> {
        self.mls_group
            .confirm_application_message(provider.storage(), epoch, generation)
            .map_err(Into::into)
    }

    /// Get a reference to the group's group id.
    pub(crate) fn group_id(&self) -> &GroupId {
        self.mls_group().group_id()
    }

    pub(crate) fn own_user_id(&self) -> &UserId {
        &self.own_user_id
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
            let credential = LeafCredential::from_credential(&m.credential)
                .inspect_err(|error| {
                    error!(%error, "Invalid member credential");
                })
                .ok()?;
            Some(credential.user_id(self.own_user_id()).clone())
        })
    }

    pub(super) async fn update(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        signer: &LeafSigningKey,
        new_group_data: Option<GroupDataBytes>,
        derivation_epoch: DerivationEpoch,
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
        let leaf_node_parameters = Self::update_leaf_node_extensions(
            own_leaf_node.extensions(),
            self.own_leaf_capabilities(),
        )?;

        // A leaf shared with sibling emulator clients must be replaced with key
        // material derived from the emulation epoch, or the siblings cannot
        // rederive it and drop out of the group.
        let vc_group_id = self.resolve_vc_emulation_group(&mut *txn).await?;

        self.mls_group.set_aad(aad);
        let (mls_message, group_info) = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());

            let mut builder = self.mls_group.commit_builder();
            if let Some(extensions) = extensions {
                builder = builder.propose_group_context_extensions(extensions)?;
            };

            let mut builder = builder
                .force_self_update(true)
                .leaf_node_parameters(leaf_node_parameters)
                .derivation_epoch(derivation_epoch.rotates());
            if let Some(group_id) = &vc_group_id {
                builder = builder.vc_emulation(provider.crypto(), provider.storage(), group_id)?;
            }

            let (mls_message, _welcome_option, group_info_option) = builder
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

    /// APQ self-update on both the T and PQ groups.
    ///
    /// Produces a single combined commit via apqmls that forces a self-update of the key material
    /// in both groups. Return [`ApqGroupOperationParamsOut`] so the caller can persist it as
    /// `ApqOther` pending chat operation.
    pub(super) async fn apq_update(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        signer: &LeafSigningKey,
        derivation_epoch: DerivationEpoch,
    ) -> anyhow::Result<ApqGroupOperationParamsOut> {
        let aad = AadMessage::from(AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys: Vec::new(),
        }))
        .tls_serialize_detached()?;
        self.mls_group.set_aad(aad);

        let t_own_leaf_node = self.mls_group.own_leaf_node().context("No own leaf node")?;
        let t_leaf_node_parameters = Self::update_leaf_node_extensions(
            t_own_leaf_node.extensions(),
            self.own_leaf_capabilities(),
        )?;
        let pq_own_leaf_node = self
            .pq()
            .context("No PQ group found")?
            .mls_group
            .own_leaf_node()
            .context("No own PQ leaf node")?;
        let pq_leaf_node_parameters = Self::update_leaf_node_extensions(
            pq_own_leaf_node.extensions(),
            self.own_leaf_capabilities(),
        )?;

        let vc_group_id = self.resolve_vc_emulation_group(&mut *txn).await?;

        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let (t_mls_group, pq_mls_group) = self.apq_mls_groups_mut()?;
        let mut builder =
            apqmls::commit_builder::CommitBuilder::from_groups(t_mls_group, pq_mls_group)
                .force_self_update(true)
                .leaf_node_parameters(t_leaf_node_parameters, pq_leaf_node_parameters)
                .derivation_epoch(derivation_epoch.rotates())
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

    /// Capabilities for the own leaf when a commit sets explicit leaf node parameters.
    ///
    /// Self-group leaves must keep advertising the self-group credential type, see
    /// [`self_group_leaf_node_capabilities`].
    fn own_leaf_capabilities(&self) -> Capabilities {
        if self.is_self_group() {
            self_group_leaf_node_capabilities()
        } else {
            default_leaf_node_capabilities()
        }
    }

    fn update_leaf_node_extensions(
        leaf_node_extensions: &Extensions<LeafNode>,
        capabilities: Capabilities,
    ) -> anyhow::Result<LeafNodeParameters> {
        let mut leaf_node_parameters =
            LeafNodeParameters::builder().with_capabilities(capabilities);

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
                // Update features to the current version of the client
                let current_features = AirFeatures::default_leaf_or_key_package_features();
                if air_component.features != current_features {
                    air_component.features = current_features;
                    updated_dict
                        .get_or_insert_with(|| dict.clone())
                        .insert(AIR_COMPONENT_ID, air_component.to_bytes()?);
                }
            } else {
                // Add air component to the app data dictionary.
                updated_dict.get_or_insert_with(|| dict.clone()).insert(
                    AIR_COMPONENT_ID,
                    AirComponent::default_for_leaf_or_key_package()
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
            leaf_node_extensions.add(default_app_data_dictionary_extension::<AirComponent>())?;
            leaf_node_parameters = leaf_node_parameters.with_extensions(leaf_node_extensions);
        }

        Ok(leaf_node_parameters.build())
    }

    pub(super) fn stage_leave_group(
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

    pub(super) fn restage_leave_group(
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
        self.compile_removed_list(self.mls_group().pending_proposals(), |index| {
            self.user_id_at_index(index)
        })
    }

    fn staged_commit_removes(
        &self,
        staged_commit: Option<&'_ StagedCommit>,
    ) -> Vec<(RoomPolicyIdentity, RoomPolicyIdentity)> {
        let Some(staged_commit) = staged_commit.or_else(|| self.mls_group().pending_commit())
        else {
            return Vec::new();
        };
        self.compile_removed_list(staged_commit.queued_proposals(), |index| {
            self.room_identity_at_index(index)
        })
    }

    /// Collects (remover, removed) pairs from remove proposals, resolving each leaf index with
    /// `resolve`. UI paths resolve to user ids, room-state paths to room-policy identities.
    fn compile_removed_list<'a, T>(
        &self,
        removes: impl Iterator<Item = &'a QueuedProposal>,
        resolve: impl Fn(LeafNodeIndex) -> Option<T>,
    ) -> Vec<(T, T)> {
        let mut pending_removes = Vec::new();

        for proposal in removes {
            let Sender::Member(remover) = proposal.sender() else {
                // We don't support external senders yet.
                continue;
            };
            let Some(remover) = resolve(*remover) else {
                continue;
            };
            if let Some(removed_client_index) = removed_client(proposal)
                && let Some(removed) = resolve(removed_client_index)
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
            LeafCredential::from_credential(&m.credential)
                .map(|c| c.user_id(self.own_user_id()).clone())
                .ok()
        })
    }

    fn room_identity_at_index(&self, index: LeafNodeIndex) -> Option<RoomPolicyIdentity> {
        self.mls_group().member_at(index).and_then(|m| {
            LeafCredential::from_credential(&m.credential)
                .ok()
                .map(|c| c.room_policy_identity())
        })
    }

    /// Returns a list of (adder, added) room-policy identity pairs for pending add proposals.
    pub(crate) fn pending_adds(
        &self,
        staged_commit: Option<&'_ StagedCommit>,
    ) -> Vec<(RoomPolicyIdentity, RoomPolicyIdentity)> {
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
            let Some(adder) = self.room_identity_at_index(*adder_index) else {
                continue;
            };
            let Ok(added_credential) = LeafCredential::from_credential(
                proposal
                    .add_proposal()
                    .key_package()
                    .leaf_node()
                    .credential(),
            ) else {
                continue;
            };
            pending_adds.push((adder, added_credential.room_policy_identity()));
        }
        pending_adds
    }

    pub(crate) fn verify_role_change_identity(
        &self,
        sender: &RoomPolicyIdentity,
        target: &RoomPolicyIdentity,
        role: RoleIndex,
    ) -> Result<()> {
        let sender = sender.to_bytes()?;
        let target = target.to_bytes()?;
        let result = self
            .room_state
            .can_apply_regular_proposals(&sender, &[MimiProposal::ChangeRole { target, role }]);

        Ok(result?)
    }

    pub(crate) fn verify_role_change(
        &self,
        sender: &UserId,
        target: &UserId,
        role: RoleIndex,
    ) -> Result<()> {
        self.verify_role_change_identity(
            &RoomPolicyIdentity::User(sender.clone()),
            &RoomPolicyIdentity::User(target.clone()),
            role,
        )
    }

    pub(crate) fn room_state_change_role_identity(
        &mut self,
        sender: &RoomPolicyIdentity,
        target: &RoomPolicyIdentity,
        role: RoleIndex,
    ) -> Result<()> {
        let sender = sender.to_bytes()?;
        let target = target.to_bytes()?;
        let result = self
            .room_state
            .apply_regular_proposals(&sender, &[MimiProposal::ChangeRole { target, role }]);

        Ok(result?)
    }

    pub(crate) fn room_state_change_role(
        &mut self,
        sender: &UserId,
        target: &UserId,
        role: RoleIndex,
    ) -> Result<()> {
        self.room_state_change_role_identity(
            &RoomPolicyIdentity::User(sender.clone()),
            &RoomPolicyIdentity::User(target.clone()),
            role,
        )
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

    /// Whether our leaf in this group is operated by a virtual client, i.e. it is
    /// shared with sibling emulator clients.
    pub(crate) fn own_leaf_is_virtual_client(&self) -> bool {
        self.mls_group()
            .own_leaf_node()
            .is_some_and(leaf_node_is_virtual_client)
    }

    /// The emulation group a commit replacing our leaf has to derive from, or
    /// `None` if this leaf is not shared with sibling emulator clients.
    ///
    /// The emulation group is the self group. openmls resolves the concrete
    /// derivation epoch from it, always taking the newest one.
    async fn resolve_vc_emulation_group(
        &self,
        connection: impl ReadConnection,
    ) -> Result<Option<GroupId>> {
        if !self.own_leaf_is_virtual_client() {
            return Ok(None);
        }
        let group_id = OwnClientInfo::load_self_group_id(connection)
            .await?
            .context("no self group to derive the emulation epoch from")?;
        Ok(Some(group_id))
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

    pub(crate) fn delete_connection_offer_psk(
        mut connection: impl WriteConnection,
        connection_offer_hash: ConnectionOfferHash,
    ) -> Result<()> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let psk = Psk::External(ExternalPsk::new(
            connection_offer_hash.into_bytes().to_vec(),
        ));
        provider.storage().delete_psk(&psk)?;
        Ok(())
    }

    /// Deserializes user credentials from the corresponding leaf node.
    ///
    /// Does not guarantee that the credential was verified and is valid.
    pub(crate) fn unverified_credential_at(
        &self,
        index: LeafNodeIndex,
    ) -> Result<Option<LeafCredential>, LeafCredentialError> {
        self.mls_group
            .member_at(index)
            .map(|m| LeafCredential::from_credential(&m.credential))
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
    ) -> anyhow::Result<Option<UserCredential>> {
        ensure!(self.group_id() == witness.group_id(), "Group ID mismatch");
        let Some(credential) = self.unverified_credential_at(index)? else {
            return Ok(None);
        };
        match credential {
            LeafCredential::User(credential) => {
                Ok(Some(UserCredential::assume_verified(credential, witness)))
            }
            LeafCredential::SelfGroup(_) => {
                Err(anyhow!("self-group leaf carries no user credential"))
            }
        }
    }

    /// Same as [`Self::credential_at`] but resolves the leaf owner's user id, which also works
    /// for self-group leaves. They carry no user credential and resolve to the own user id.
    pub(crate) fn user_id_at(
        &self,
        index: LeafNodeIndex,
        witness: &impl GroupStorageWitness,
    ) -> anyhow::Result<Option<UserId>> {
        ensure!(self.group_id() == witness.group_id(), "Group ID mismatch");
        let Some(credential) = self.unverified_credential_at(index)? else {
            return Ok(None);
        };
        Ok(Some(credential.user_id(self.own_user_id()).clone()))
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
    // Whether this is the user's own self group. A self-group credential carries nothing to
    // verify against the AS, so it is only accepted here.
    is_self_group: bool,
) -> anyhow::Result<Vec<StorableUserCredential>> {
    let unverified_credentials = classify_member_credentials(
        mls_group.members().map(|member| {
            (
                member.credential,
                SignaturePublicKey::from(member.signature_key),
            )
        }),
        is_self_group,
    )?;

    let as_credentials = AsCredentials::fetch_for_verification(
        txn,
        api_clients,
        unverified_credentials.iter().map(|(c, _)| c),
    )
    .await?;

    let mut verified = Vec::with_capacity(unverified_credentials.len());
    for (credential, leaf_verifying_key) in unverified_credentials {
        let credential = VerifiableUserCredential::verify_and_validate(
            credential,
            &leaf_verifying_key,
            None,
            &as_credentials,
        )?;
        verified.push(credential);
    }
    Ok(verified)
}

/// Ensure that every user of `room_sate` is a member of `mls_group`.
///
/// Members missing from the room state are tolerated: the DS does not add external joiners of
/// connection groups to its room state, clients patch that locally.
fn ensure_room_state_users_are_members(
    room_state: &VerifiedRoomState,
    mls_group: &MlsGroup,
) -> anyhow::Result<()> {
    let mut members = HashSet::new();
    for member in mls_group.members() {
        let identity = LeafCredential::from_credential(&member.credential)?
            .room_policy_identity()
            .to_bytes()?;
        members.insert(identity);
    }
    let users = room_state.users();
    ensure!(
        users.keys().all(|identity| members.contains(identity)),
        "room state lists users which are not group members"
    );
    if users.len() < members.len() {
        warn!(
            group_id = ?mls_group.group_id(),
            "room state is missing group members",
        );
    }
    Ok(())
}

/// Classify the leaf credentials of all group members for verification.
///
/// User credentials are returned together with their leaf signature keys for AS verification.
/// Self-group credentials carry nothing to verify against the AS. They are only accepted inside
/// the user's own self-group, where room policy is keyed on the client id, so each leaf must
/// carry a distinct one. Conversely, the self-group accepts only self-group credentials.
fn classify_member_credentials(
    members: impl Iterator<Item = (Credential, SignaturePublicKey)>,
    is_self_group: bool,
) -> anyhow::Result<Vec<(VerifiableUserCredential, SignaturePublicKey)>> {
    let mut client_ids = HashSet::new();
    let mut unverified_credentials = Vec::new();
    for (credential, signature_key) in members {
        match LeafCredential::from_credential(&credential) {
            Ok(LeafCredential::User(credential)) => {
                ensure!(!is_self_group, "user credential in the self-group");
                unverified_credentials.push((credential, signature_key));
            }
            Ok(LeafCredential::SelfGroup(credential)) => {
                ensure!(
                    is_self_group,
                    "self-group credential outside the self-group"
                );
                ensure!(
                    client_ids.insert(credential.client_id()),
                    "duplicate client id in the self-group"
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(unverified_credentials)
}

/// Validate the leaf credential of a client about to be added to the self-group.
///
/// The credential must be a self-group credential and its client id must not collide with a
/// leaf already in the self group, since room policy is keyed on the client id.
fn validate_self_group_add_credential(
    members: impl Iterator<Item = Credential>,
    added: &Credential,
) -> anyhow::Result<()> {
    let LeafCredential::SelfGroup(added) = LeafCredential::from_credential(added)? else {
        bail!("expected a self-group credential");
    };
    for credential in members {
        match LeafCredential::from_credential(&credential)? {
            LeafCredential::SelfGroup(existing) => {
                ensure!(
                    existing.client_id() != added.client_id(),
                    "client id already present in the self-group"
                );
            }
            LeafCredential::User(_) => bail!("user credential in the self-group"),
        }
    }
    Ok(())
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
        chat.set_status(&mut *txn, ChatStatus::inactive(past_members))
            .await?;
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
    use aircommon::codec::PersistenceCodec;
    use anyhow::Context as _;
    use chrono::{DateTime, Utc};

    use crate::{Chat, ChatId, clients::CoreUser};

    impl CoreUser {
        /// Replaces the payload of the chat group's persisted MLS state with
        /// bytes that fail to decode, reproducing a group whose stored state we
        /// can no longer read, e.g. after a serialization format change.
        ///
        /// The codec version byte is kept, so the corruption survives a codec
        /// version bump.
        pub async fn corrupt_mls_group_state(&self, chat_id: ChatId) -> anyhow::Result<()> {
            let group_id = self
                .db()
                .with_read_transaction(async |txn| Chat::load(txn, &chat_id).await)
                .await?
                .with_context(|| format!("Can't find chat with id {chat_id}"))?
                .group_id()
                .clone();
            // The MLS storage provider keys its rows by the encoded group id.
            let group_id = PersistenceCodec::to_vec(&group_id)?;

            let mut connection = self.db().write().await?;
            // Concatenation yields TEXT, so the result is cast back to a blob to keep the
            // failure in the codec rather than in the column type.
            sqlx::query(
                "UPDATE group_data
                SET group_data = CAST(substr(group_data, 1, 1) || x'FF' AS BLOB)
                WHERE group_id = ?1",
            )
            .bind(group_id)
            .execute(connection.as_mut())
            .await?;

            Ok(())
        }

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

        pub async fn pq_self_updated_at(
            &self,
            chat_id: ChatId,
        ) -> sqlx::Result<Option<DateTime<Utc>>> {
            Chat::pq_self_updated_at(self.db().read().await?, chat_id).await
        }

        pub async fn set_pq_self_updated_at(
            &self,
            chat_id: ChatId,
            self_updated_at: DateTime<Utc>,
        ) -> sqlx::Result<()> {
            Chat::set_pq_self_updated_at(self.db().write().await?, chat_id, self_updated_at).await
        }
    }
}

#[cfg(feature = "test_utils")]
impl Group {
    /// Creates a self-update commit forcing a specific [`AirComponent`] into the leaf node.
    ///
    /// Useful for simulating old clients that lack certain feature flags.
    pub(crate) async fn update_with_air_component(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        signer: &LeafSigningKey,
        air_component: AirComponent,
    ) -> Result<GroupOperationParamsOut> {
        let aad = AadMessage::from(AadPayload::GroupOperation(GroupOperationParamsAad {
            new_encrypted_user_profile_keys: Vec::new(),
        }))
        .tls_serialize_detached()?;

        let own_leaf_node = self.mls_group.own_leaf_node().context("No own leaf node")?;
        let leaf_node_parameters = Self::forced_air_component_leaf_params(
            own_leaf_node.extensions(),
            self.own_leaf_capabilities(),
            air_component,
        )?;

        self.mls_group.set_aad(aad);
        let (mls_message, group_info) = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            let (mls_message, _welcome_option, group_info_option) = self
                .mls_group
                .commit_builder()
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

    fn forced_air_component_leaf_params(
        leaf_node_extensions: &Extensions<LeafNode>,
        capabilities: Capabilities,
        air_component: AirComponent,
    ) -> anyhow::Result<LeafNodeParameters> {
        let mut leaf_node_parameters =
            LeafNodeParameters::builder().with_capabilities(capabilities);

        let mut dict = leaf_node_extensions
            .app_data_dictionary()
            .map(|e| e.dictionary().clone())
            .unwrap_or_default();

        // Ensure AppComponents entry is present
        if dict.get(&ComponentType::AppComponents.into()).is_none() {
            dict.insert(
                ComponentType::AppComponents.into(),
                ComponentsList {
                    component_ids: SUPPORTED_COMPONENTS.to_vec(),
                }
                .tls_serialize_detached()?,
            );
        }
        // Force the given air component, overriding whatever was there before
        dict.insert(AIR_COMPONENT_ID, air_component.to_bytes()?);

        let mut new_leaf_node_extensions = leaf_node_extensions.clone();
        new_leaf_node_extensions.add_or_replace(Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dict),
        ))?;
        leaf_node_parameters = leaf_node_parameters.with_extensions(new_leaf_node_extensions);
        Ok(leaf_node_parameters.build())
    }
}

#[cfg(test)]
mod member_credential_validation_tests {
    use aircommon::{
        credentials::{SelfGroupCredential, test_utils::create_test_credentials},
        identifiers::UserId,
    };
    use uuid::Uuid;

    use super::*;

    fn self_group_credential(client_id: Uuid) -> Credential {
        SelfGroupCredential::new(client_id)
            .to_credential()
            .expect("serializing a self-group credential")
    }

    fn user_credential() -> Credential {
        let user_id = UserId::random("example.com".parse().unwrap());
        let (_as_signing_key, user_signing_key) = create_test_credentials(user_id);
        Credential::try_from(user_signing_key.credential()).expect("serializing a user credential")
    }

    fn signature_key() -> SignaturePublicKey {
        SignaturePublicKey::from(vec![0u8; 32])
    }

    #[test]
    fn self_group_members_with_unique_client_ids_is_accepted() {
        let members = [
            (self_group_credential(Uuid::from_u128(1)), signature_key()),
            (self_group_credential(Uuid::from_u128(2)), signature_key()),
        ];
        let unverified = classify_member_credentials(members.into_iter(), true)
            .expect("unique client ids should be accepted");
        assert!(unverified.is_empty());
    }

    #[test]
    fn duplicate_client_ids_in_self_group_are_rejected() {
        let client_id = Uuid::from_u128(1);
        let members = [
            (self_group_credential(client_id), signature_key()),
            (self_group_credential(client_id), signature_key()),
        ];
        let error = classify_member_credentials(members.into_iter(), true)
            .expect_err("duplicate client ids should be rejected");
        assert!(
            error.to_string().contains("duplicate client id"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn self_group_credential_outside_self_group_is_rejected() {
        let members = [(self_group_credential(Uuid::from_u128(1)), signature_key())];
        let error = classify_member_credentials(members.into_iter(), false)
            .expect_err("self-group credential outside the self-group should be rejected");
        assert!(
            error.to_string().contains("outside the self-group"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn user_credential_in_self_group_is_rejected() {
        let members = [
            (user_credential(), signature_key()),
            (self_group_credential(Uuid::from_u128(1)), signature_key()),
        ];
        let error = classify_member_credentials(members.into_iter(), true)
            .expect_err("user credential in the self-group should be rejected");
        assert!(
            error.to_string().contains("user credential"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn adding_a_fresh_client_id_is_accepted() {
        let members = [self_group_credential(Uuid::from_u128(1))];
        let added = self_group_credential(Uuid::from_u128(2));
        validate_self_group_add_credential(members.into_iter(), &added)
            .expect("fresh client id should be accepted");
    }

    #[test]
    fn adding_a_duplicate_client_id_is_rejected() {
        let client_id = Uuid::from_u128(1);
        let members = [self_group_credential(client_id)];
        let added = self_group_credential(client_id);
        let error = validate_self_group_add_credential(members.into_iter(), &added)
            .expect_err("duplicate client id should be rejected");
        assert!(
            error.to_string().contains("already present"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn adding_a_user_credential_is_rejected() {
        let members = [self_group_credential(Uuid::from_u128(1))];
        let added = user_credential();
        let error = validate_self_group_add_credential(members.into_iter(), &added)
            .expect_err("user credential should be rejected");
        assert!(
            error
                .to_string()
                .contains("expected a self-group credential"),
            "unexpected error: {error:#}"
        );
    }
}

#[cfg(test)]
mod handle_group_not_found_tests {
    use aircommon::{
        credentials::test_utils::create_test_credentials,
        identifiers::{QsClientId, QsUserId, QualifiedGroupId, UserId},
    };
    use sqlx::query;
    use uuid::Uuid;

    use crate::{
        Chat, ChatStatus,
        clients::{block_contact::BlockedContact, own_client_info::OwnClientInfo},
        db::access::DbAccess,
        groups::GroupDataBytes,
        utils::persistence::open_db_in_memory,
    };

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn handle_group_not_found_marks_blocked_chat_inactive_under_block() -> anyhow::Result<()>
    {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;

        let own_user_id = UserId::random("example.com".parse().unwrap());
        let blocked_user_id = UserId::random("example.com".parse().unwrap());
        let (_as_signing_key, user_signing_key) = create_test_credentials(own_user_id);

        let qgid = QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse().unwrap());
        let group_id = GroupId::from(qgid);

        let (group, _) = Group::create_group(
            &mut connection,
            &user_signing_key,
            IdentityLinkWrapperKey::random()?,
            group_id.clone(),
            GroupDataBytes::from(b"test-group-data".to_vec()),
        )?;
        group.store(&mut connection).await?;

        // Loading a group resolves the owner's identity, which requires an own_client_info row.
        OwnClientInfo {
            qs_user_id: QsUserId::random(),
            qs_client_id: QsClientId::random(&mut rand::rng()),
            user_id: user_signing_key.credential().user_id().clone(),
            client_id: Uuid::new_v4(),
            self_group_id: None,
            self_group_signing_key: None,
        }
        .store(&mut connection)
        .await?;

        let chat = Chat::new_targeted_message_chat(group_id.clone(), blocked_user_id.clone());
        let chat_id = chat.id();
        chat.store(&mut connection).await?;

        BlockedContact::new(blocked_user_id.clone())
            .store(&mut connection)
            .await?;

        let mut txn = connection.begin().await?;

        assert!(matches!(
            Chat::load(&mut txn, &chat_id)
                .await?
                .expect("chat should exist")
                .status(),
            ChatStatus::Blocked
        ));

        handle_group_not_found_on_ds(&mut txn, &group_id).await?;

        assert!(Group::load(&mut txn, &group_id).await?.is_none());
        assert!(matches!(
            Chat::load(&mut txn, &chat_id)
                .await?
                .expect("chat should still exist")
                .status(),
            ChatStatus::Blocked
        ));

        query("DELETE FROM blocked_contact WHERE user_uuid = ?1 AND user_domain = ?2")
            .bind(blocked_user_id.uuid())
            .bind(blocked_user_id.domain().to_string())
            .execute(txn.as_mut())
            .await?;

        assert!(matches!(
            Chat::load(&mut txn, &chat_id)
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
                .user_id_at(*sender_index, verified)?
                .context("Could not find user credential of message sender")?;

            let Some(removed_index) = removed_client(remove_proposal) else {
                // This cannot happen since we filtered for remove proposals.
                continue;
            };

            let removed = group
                .user_id_at(removed_index, verified)?
                .context("Could not find user credential of removed")?;

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
                .user_id_at(*sender_index, verified)?
                .context("Could not find user credential of sender")?;

            // Get the user id of the added member from the proposal key package
            let credential = staged_add_proposal
                .add_proposal()
                .key_package()
                .leaf_node()
                .credential();
            let credential = LeafCredential::from_credential(credential)?;
            let addee_id = credential.user_id(group.own_user_id()).clone();

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
                let user_id = group
                    .user_id_at(*sender_index, verified)?
                    .context("Could not find user credential of sender")?;
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

/// Verifies that every leaf holds the same signature key in both legs.
///
/// This is the binding between a member's two leaves in our deployment: the PQ
/// leaf carries an empty credential, so the signing key is the only thing
/// tying the two together. The draft requires membership to be consistent
/// across the two sessions without saying how a member is identified, so this
/// is our policy rather than a protocol rule, which is why it lives here and
/// not in `apqmls`.
///
/// Only usable while both ciphersuites agree on the signature algorithm, which
/// holds for every ciphersuite pair we deploy. See
/// [`Group::verify_pq_signature_key_at`] for the per-operation counterpart
/// applied to incoming commits.
fn verify_pq_signature_keys(t_group: &MlsGroup, pq_group: &MlsGroup) -> anyhow::Result<()> {
    ensure!(
        t_group.ciphersuite().signature_algorithm() == pq_group.ciphersuite().signature_algorithm(),
        "the two legs must share a signature algorithm to be bound by their signing keys"
    );
    let pq_keys: HashMap<_, _> = pq_group
        .members()
        .map(|member| (member.index, member.signature_key))
        .collect();
    for t_member in t_group.members() {
        let pq_key = pq_keys
            .get(&t_member.index)
            .with_context(|| format!("no PQ leaf at index {:?}", t_member.index))?;
        ensure!(
            &t_member.signature_key == pq_key,
            "T and PQ signature keys at index {:?} do not match",
            t_member.index
        );
    }
    Ok(())
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
    use aircommon::mls_group_config::{
        default_app_data_dictionary_extension, default_leaf_node_capabilities,
    };
    use airprotos::client::component::{AIR_COMPONENT_ID, AirComponent};
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
        let params =
            Group::update_leaf_node_extensions(&extensions, default_leaf_node_capabilities())
                .unwrap();
        let ids = air_component_ids(params.extensions().unwrap()).unwrap();
        assert!(ids.contains(&AIR_COMPONENT_ID));
    }

    /// App data dictionary present but no AppComponents key -> add AppComponents with AIR_COMPONENT_ID
    #[test]
    fn app_data_dictionary_without_app_components() {
        let extensions = extensions_with_dict(AppDataDictionary::new());
        let params =
            Group::update_leaf_node_extensions(&extensions, default_leaf_node_capabilities())
                .unwrap();
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
        let params =
            Group::update_leaf_node_extensions(&extensions, default_leaf_node_capabilities())
                .unwrap();
        let ids = air_component_ids(params.extensions().unwrap()).unwrap();
        assert!(ids.contains(&AIR_COMPONENT_ID));
        assert!(ids.contains(&other_id));
    }

    /// AIR_COMPONENT_ID already present -> extensions in params are unchanged (None)
    #[test]
    fn app_components_with_air_component_id_already() {
        let extensions =
            Extensions::from_vec(vec![default_app_data_dictionary_extension::<AirComponent>()])
                .expect("valid extensions");
        let params =
            Group::update_leaf_node_extensions(&extensions, default_leaf_node_capabilities())
                .unwrap();
        assert!(params.extensions().is_none());
    }
}
