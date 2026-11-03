// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use aircommon::{
    credentials::keys::{ClientKeyType, SelfGroupSigningKey, UserSigningKey},
    crypto::{
        aead::{
            AeadDecryptable,
            keys::{EncryptedUserProfileKey, WelcomeAttributionInfoEarKey},
        },
        hpke::{HpkeDecryptable, JoinerInfoDecryptionKey},
        signatures::{private_keys::SigningKey, signable::Verifiable},
    },
    identifiers::{QualifiedGroupId, UserId},
    messages::{
        client_ds::{ApqWelcomeBundle, DsJoinerInformation, WelcomeBundle},
        client_ds_out::WelcomeInfoIn,
        welcome_attribution_info::{WelcomeAttributionInfo, WelcomeAttributionInfoPayload},
    },
    mls_group_config::default_mls_group_join_config,
};
use anyhow::{Context, Result, bail, ensure};
use apqmls::validation::{validate_apq_session_at_construction, validate_welcome_psk};
use openmls::{
    group::{JoinBuilder, ProcessedWelcome},
    prelude::{LeafNodeIndex, SignaturePublicKey, tls_codec::Serialize as TlsSerializeTrait},
};

use crate::{
    clients::{
        api_clients::ApiClients,
        block_contact::{BlockedContact, BlockedContactError},
        own_client_info::OwnClientInfo,
    },
    db::access::WriteDbTransaction,
    groups::{
        Group,
        client_auth_info::StorableUserCredential,
        credentials::{verify_member_credentials, verify_pq_signature_keys},
        error::GroupOperationError,
        join::{DecryptedProfileInfos, ensure_self_group_flag},
        openmls_provider::AirOpenMlsProvider,
        room_state::ensure_room_state_matches_members,
    },
};

/// Candidate signing keys for the DS `welcome_info` lookups when joining a
/// group. The right key is the one matching the joiner's leaf credential.
pub(crate) struct JoinSigners<'a> {
    /// The shared user signing key. Key packages derived from a sibling's
    /// upload always carry the shared user credential.
    pub(crate) client: &'a UserSigningKey,
    /// The self-group signing key, if provisioned. A freshly linked device
    /// joins the self-group with it.
    pub(crate) self_group: Option<&'a SelfGroupSigningKey>,
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

impl Group {
    /// Join a group with the provided welcome message. If there exists a group
    /// with the same ID, checks if that group is inactive and if so deletes the
    /// old group before it stores the new one.
    ///
    /// Returns the group name, sender user id and the list of profile keys of the members.
    pub(crate) async fn join_group(
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

        let sender_user_id = verifiable_attribution_info.sender();
        if BlockedContact::check_blocked(&mut *txn, &sender_user_id).await? {
            bail!(BlockedContactError);
        }

        // Self-groups are only ever joined during device linking, which uses the APQ join path.
        ensure_self_group_flag(&mls_group, None, false)?;

        let credentials =
            verify_member_credentials(&mut *txn, api_clients, &mls_group, false).await?;
        ensure_room_state_matches_members(&room_state, &mls_group)?;

        let sender_user_credential =
            load_sender_credential(&mut *txn, &sender_user_id, &credentials).await?;
        let welcome_attribution_info: WelcomeAttributionInfoPayload =
            verifiable_attribution_info.verify(sender_user_credential.verifying_key())?;

        let group = Self::new(
            welcome_attribution_info.into_identity_link_wrapper_key(),
            joiner_info.group_state_ear_key,
            mls_group,
            None,
            room_state,
            signer.credential().user_id().clone(),
        );

        // Phase 6: Store the group and user credentials, and decrypt profile keys.
        let encrypted_profile_keys_fallback = profile_keys_fallback_by_credentials(
            &credentials,
            encrypted_user_profile_keys,
            &indexed_encrypted_user_profile_keys,
        );
        let member_profile_info = group
            .store_after_join(
                txn,
                credentials,
                indexed_encrypted_user_profile_keys,
                encrypted_profile_keys_fallback,
            )
            .await?;

        Ok((group, sender_user_id, member_profile_info))
    }

    /// Same as [`Self::join_group`], but for APQ groups.
    pub(crate) async fn join_apq_group(
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
        ensure_self_group_flag(&t_mls_group, Some(&pq_mls_group), is_self_group)?;
        let credentials =
            verify_member_credentials(txn, api_clients, &t_mls_group, is_self_group).await?;
        ensure_room_state_matches_members(&room_state, &t_mls_group)?;

        let sender_user_credential =
            load_sender_credential(&mut *txn, &sender_user_id, &credentials).await?;
        let welcome_attribution_info: WelcomeAttributionInfoPayload =
            verifiable_attribution_info.verify(sender_user_credential.verifying_key())?;

        let group = Self::new(
            welcome_attribution_info.into_identity_link_wrapper_key(),
            joiner_info.group_state_ear_key,
            t_mls_group,
            Some(pq_mls_group),
            room_state,
            signers.client.credential().user_id().clone(),
        );
        let encrypted_profile_keys_fallback = profile_keys_fallback_by_credentials(
            &credentials,
            encrypted_user_profile_keys,
            &indexed_encrypted_user_profile_keys,
        );
        let member_profile_info = group
            .store_after_join(
                txn,
                credentials,
                indexed_encrypted_user_profile_keys,
                encrypted_profile_keys_fallback,
            )
            .await?;

        Ok((group, sender_user_id, member_profile_info))
    }
}

/// The stored credential of the welcome's sender.
///
/// A linked device may not know the inviter yet. Since the inviter is a member, fall back to its
/// AS-verified member credential.
async fn load_sender_credential(
    txn: &mut WriteDbTransaction<'_>,
    sender_user_id: &UserId,
    credentials: &[StorableUserCredential],
) -> Result<StorableUserCredential> {
    match StorableUserCredential::load_by_user_id(&mut *txn, sender_user_id).await? {
        Some(credential) => Ok(credential),
        None => credentials
            .iter()
            .find(|c| c.user_id() == sender_user_id)
            .cloned()
            .context("sender is not a member of the group"),
    }
}

/// Positional fallback (user id -> encrypted profile key) for servers that don't send indexed
/// keys yet. The keys are in the order of the member credentials.
fn profile_keys_fallback_by_credentials(
    credentials: &[StorableUserCredential],
    encrypted_user_profile_keys: Vec<EncryptedUserProfileKey>,
    indexed_encrypted_user_profile_keys: &HashMap<LeafNodeIndex, EncryptedUserProfileKey>,
) -> HashMap<UserId, EncryptedUserProfileKey> {
    if !indexed_encrypted_user_profile_keys.is_empty() {
        return HashMap::new();
    }
    credentials
        .iter()
        .map(|c| c.user_id().clone())
        .zip(encrypted_user_profile_keys)
        .collect()
}
