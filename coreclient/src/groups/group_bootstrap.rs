// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Sealing and opening of group bootstrap blobs on the self group.
//!
//! When one emulator client creates a group or joins one via external commit,
//! it hands its siblings the group's keys and connection context in a
//! `GroupBootstrap` (see `airprotos::client::group_bootstrap`). The blob key
//! is derived from one application-ratchet generation of the self group's
//! newest emulation epoch, whose coordinates travel in the clear inside the
//! blob.

use aircommon::{
    codec::PersistenceCodec,
    crypto::{
        aead::{
            AEAD_KEY_SIZE,
            keys::{
                FriendshipPackageEarKey, GroupBootstrapKey, GroupStateEarKey,
                IdentityLinkWrapperKey,
            },
        },
        indexed_aead::keys::TypedSecret,
        kdf::{KdfDerivable, keys::VcApplicationSecret},
        secrets::Secret,
    },
    identifiers::{UserId, Username},
    messages::client_as::ConnectionOfferHash,
};
use airprotos::client::group_bootstrap::{
    ConnectionContext, GroupBootstrap, GroupBootstrapBlob, GroupBootstrapCarrier,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use openmls::{components::vc_derivation_info::EpochId, prelude::GroupId};

use crate::{
    clients::connection_offer::FriendshipPackage,
    db::access::WriteDbTransaction,
    groups::{Group, openmls_provider::AirOpenMlsProvider, self_group::SelfGroup},
};

/// Domain separation of the application-ratchet derivation that feeds the
/// group bootstrap key.
const OPERATION_CONTEXT: &[u8] = b"group bootstrap";

impl Group {
    /// Seals `bootstrap` for the sibling emulator clients of this self group.
    ///
    /// Errors if called on a group that is not the self group.
    pub(crate) fn seal_group_bootstrap(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        bootstrap: &GroupBootstrap,
        carrier: GroupBootstrapCarrier,
        group_id: &GroupId,
    ) -> Result<GroupBootstrapBlob> {
        ensure!(
            self.is_self_group(),
            "seal_group_bootstrap must only be called on the self group"
        );
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let (info, secret) = self
            .mls_group()
            .next_vc_application_secret(&provider, OPERATION_CONTEXT)?;
        let key = bootstrap_key(secret)?;
        let sealed = bootstrap.seal(&key, carrier, group_id)?;
        Ok(GroupBootstrapBlob::new(&info, sealed))
    }

    /// Opens a blob a sibling emulator client sealed on this self group, and
    /// returns it along with the emulation epoch the sibling derived from.
    ///
    /// Errors if called on a group that is not the self group, or if the blob
    /// names this client's own ratchet.
    pub(crate) fn open_group_bootstrap(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        blob: &GroupBootstrapBlob,
        carrier: GroupBootstrapCarrier,
        group_id: &GroupId,
    ) -> Result<(GroupBootstrap, EpochId)> {
        ensure!(
            self.is_self_group(),
            "open_group_bootstrap must only be called on the self group"
        );
        let info = blob
            .secret_info()
            .context("group bootstrap blob without secret coordinates")?;
        let ciphertext = blob
            .encrypted_bootstrap
            .as_ref()
            .context("group bootstrap blob without ciphertext")?;
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let secret =
            self.mls_group()
                .derive_vc_application_secret(&provider, &info, OPERATION_CONTEXT)?;
        let key = bootstrap_key(secret)?;
        let bootstrap = GroupBootstrap::open(&key, ciphertext, carrier, group_id)?;
        Ok((bootstrap, info.epoch_id))
    }
}

impl SelfGroup {
    /// Builds the bootstrap blob describing `group` and encodes it for the
    /// `group_bootstrap` request parameter of the create-group or
    /// join-connection-group request that installs `group` on the DS.
    pub(crate) fn seal_group_bootstrap_param(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        carrier: GroupBootstrapCarrier,
        connection: Option<ConnectionContext>,
    ) -> Result<Vec<u8>> {
        let bootstrap = group_bootstrap_payload(group, connection);
        let blob = self
            .group()
            .seal_group_bootstrap(txn, &bootstrap, carrier, group.group_id())?;
        Ok(PersistenceCodec::to_vec(&blob)?)
    }
}

/// The payload a sibling needs to install `group` beyond the MLS material it
/// fetches from the DS epoch snapshot.
fn group_bootstrap_payload(group: &Group, connection: Option<ConnectionContext>) -> GroupBootstrap {
    GroupBootstrap {
        group_id: Some(group.group_id().as_slice().to_vec()),
        pq_group_id: group.pq_group_id().map(|id| id.as_slice().to_vec()),
        group_state_ear_key: Some(secret_bytes(group.group_state_ear_key())),
        identity_link_wrapper_key: Some(secret_bytes(group.identity_link_wrapper_key())),
        connection,
    }
}

/// The raw bytes of a 32-byte secret.
pub(crate) fn secret_bytes(secret: &impl AsRef<Secret<AEAD_KEY_SIZE>>) -> Vec<u8> {
    secret.as_ref().secret().to_vec()
}

/// A [`GroupBootstrap`] payload whose required fields are present and of the
/// right shape.
#[derive(Debug)]
pub(crate) struct GroupBootstrapContents {
    pub(crate) group_id: GroupId,
    pub(crate) pq_group_id: Option<GroupId>,
    pub(crate) group_state_ear_key: GroupStateEarKey,
    pub(crate) identity_link_wrapper_key: IdentityLinkWrapperKey,
    /// Absent for group chats, whose attributes come from the group data
    /// extension in the snapshot's `GroupInfo` instead.
    pub(crate) connection: Option<BootstrapConnection>,
}

impl TryFrom<GroupBootstrap> for GroupBootstrapContents {
    type Error = anyhow::Error;

    fn try_from(bootstrap: GroupBootstrap) -> Result<Self> {
        let GroupBootstrap {
            group_id,
            pq_group_id,
            group_state_ear_key,
            identity_link_wrapper_key,
            connection,
        } = bootstrap;
        Ok(Self {
            group_id: GroupId::from_slice(&group_id.context("group bootstrap without a group id")?),
            pq_group_id: pq_group_id.map(|id| GroupId::from_slice(&id)),
            group_state_ear_key: secret_field(group_state_ear_key, "group state ear key")?,
            identity_link_wrapper_key: secret_field(
                identity_link_wrapper_key,
                "identity link wrapper key",
            )?,
            connection: connection.map(BootstrapConnection::try_from).transpose()?,
        })
    }
}

/// The contact context a sibling needs to mirror a connection chat.
#[derive(Debug)]
pub(crate) enum BootstrapConnection {
    /// A sibling initiated the connection via a user handle.
    HandleInitiator {
        username: Username,
        friendship_package_ear_key: FriendshipPackageEarKey,
        connection_offer_hash: ConnectionOfferHash,
    },
    /// A sibling initiated the connection via a targeted message.
    TargetedInitiator {
        user_id: UserId,
        friendship_package_ear_key: FriendshipPackageEarKey,
    },
    /// A sibling accepted a connection request by externally joining the
    /// peer's connection group.
    Accept {
        user_id: UserId,
        friendship_package: FriendshipPackage,
        /// Absent when the offer arrived as a targeted message, which carries
        /// no connection-offer PSK.
        connection_offer_hash: Option<ConnectionOfferHash>,
    },
}

impl TryFrom<ConnectionContext> for BootstrapConnection {
    type Error = anyhow::Error;

    fn try_from(context: ConnectionContext) -> Result<Self> {
        match context {
            ConnectionContext::HandleInitiator(context) => Ok(Self::HandleInitiator {
                username: Username::new(
                    context
                        .username
                        .context("handle initiator context without a username")?,
                )?,
                friendship_package_ear_key: secret_field(
                    context.friendship_package_ear_key,
                    "friendship package ear key",
                )?,
                connection_offer_hash: context
                    .connection_offer_hash
                    .context("handle initiator context without a connection offer hash")?,
            }),
            ConnectionContext::TargetedInitiator(context) => Ok(Self::TargetedInitiator {
                user_id: context
                    .user_id
                    .context("targeted initiator context without a user id")?
                    .try_into()?,
                friendship_package_ear_key: secret_field(
                    context.friendship_package_ear_key,
                    "friendship package ear key",
                )?,
            }),
            ConnectionContext::Accept(context) => Ok(Self::Accept {
                user_id: context
                    .user_id
                    .context("accept context without a user id")?
                    .try_into()?,
                friendship_package: FriendshipPackage {
                    friendship_token: context
                        .friendship_token
                        .context("accept context without a friendship token")?,
                    wai_ear_key: secret_field(context.wai_ear_key, "wai ear key")?,
                    user_profile_base_secret: secret_field(
                        context.user_profile_base_secret,
                        "user profile base secret",
                    )?,
                },
                connection_offer_hash: context.connection_offer_hash,
            }),
            ConnectionContext::Unknown => {
                bail!("group bootstrap carries an unknown connection context")
            }
        }
    }
}

/// Turns a raw 32-byte blob field into its secret type, rejecting an absent
/// field or a wrong length.
fn secret_field<KT, ST, const N: usize>(
    bytes: Option<Vec<u8>>,
    field: &str,
) -> Result<TypedSecret<KT, ST, N>> {
    let bytes = bytes.with_context(|| format!("group bootstrap without a {field}"))?;
    let bytes: [u8; N] = bytes
        .try_into()
        .map_err(|_| anyhow!("group bootstrap {field} has the wrong length"))?;
    Ok(TypedSecret::from(Secret::from(bytes)))
}

fn bootstrap_key(secret: Vec<u8>) -> Result<GroupBootstrapKey> {
    let secret: [u8; AEAD_KEY_SIZE] = secret
        .try_into()
        .map_err(|_| anyhow!("unexpected vc application secret length"))?;
    let secret = VcApplicationSecret::from_bytes(secret);
    // `secret` is zeroized as it is dropped (its inner secret is
    // `ZeroizeOnDrop`).
    Ok(GroupBootstrapKey::derive(&secret, &Vec::new())?)
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::{
            keys::{LeafSigningKey, SelfGroupSigningKey},
            test_utils::create_test_credentials,
        },
        crypto::aead::keys::IdentityLinkWrapperKey,
        identifiers::{QualifiedGroupId, UserId},
        mls_group_config::AppComponent,
    };
    use aircommon::{crypto::aead::AEAD_KEY_SIZE, messages::FriendshipToken};
    use airprotos::client::{
        component::AirComponent,
        group_bootstrap::{AcceptContext, HandleInitiatorContext, PeerUserId},
    };
    use uuid::Uuid;

    use crate::{
        db::access::{DbAccess, WriteConnection, WriteDbTransaction},
        groups::GroupDataBytes,
        utils::persistence::open_db_in_memory,
    };

    use super::*;

    fn random_group_id() -> GroupId {
        GroupId::from(QualifiedGroupId::new(
            Uuid::new_v4(),
            "example.com".parse().unwrap(),
        ))
    }

    /// Creates a fresh single-member APQ group. When `is_self_group` is set,
    /// the group is created as an emulation group, so it has a derivation
    /// epoch to seal from.
    fn create_group(
        txn: &mut WriteDbTransaction<'_>,
        is_self_group: bool,
    ) -> anyhow::Result<Group> {
        create_group_with_vc_emulation(txn, is_self_group, None)
    }

    fn create_group_with_vc_emulation(
        txn: &mut WriteDbTransaction<'_>,
        is_self_group: bool,
        vc_group_id: Option<&GroupId>,
    ) -> anyhow::Result<Group> {
        let user_id = UserId::random("example.com".parse()?);
        let signer = if is_self_group {
            LeafSigningKey::SelfGroup(SelfGroupSigningKey::generate(Uuid::new_v4())?)
        } else {
            let (_as_key, client_signer) = create_test_credentials(user_id.clone());
            LeafSigningKey::User(client_signer)
        };
        let air_component = if is_self_group {
            AirComponent::default_for_self_group()
        } else {
            AirComponent::default_for_leaf_or_key_package()
        };
        let (group, _params) = Group::create_apq_group(
            &mut *txn,
            &signer,
            user_id,
            IdentityLinkWrapperKey::random()?,
            random_group_id(),
            random_group_id(),
            GroupDataBytes::from(b"test-group-data".to_vec()),
            None,
            air_component,
            vc_group_id,
        )?;
        Ok(group)
    }

    fn carried_group_id() -> GroupId {
        GroupId::from_slice(b"carried-group-id")
    }

    fn sample_bootstrap(group_id: &GroupId) -> GroupBootstrap {
        GroupBootstrap {
            group_id: Some(group_id.as_slice().to_vec()),
            pq_group_id: None,
            group_state_ear_key: None,
            identity_link_wrapper_key: None,
            connection: None,
        }
    }

    fn peer_user_id() -> UserId {
        UserId::new(Uuid::new_v4(), "example.com".parse().unwrap())
    }

    fn accept_context(user_id: &UserId) -> AcceptContext {
        AcceptContext {
            user_id: Some(user_id.clone().into()),
            friendship_token: Some(FriendshipToken::from_bytes(vec![1; 32])),
            wai_ear_key: Some(vec![2; AEAD_KEY_SIZE]),
            user_profile_base_secret: Some(vec![3; AEAD_KEY_SIZE]),
            connection_offer_hash: Some(ConnectionOfferHash::from_bytes([4u8; 32])),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn payload_round_trips_through_the_sibling_parser() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let self_group = SelfGroup::new_for_test(create_group(&mut txn, true)?);
        let created = create_group_with_vc_emulation(&mut txn, false, Some(self_group.group_id()))?;

        let peer = peer_user_id();
        let payload = group_bootstrap_payload(
            &created,
            Some(ConnectionContext::Accept(accept_context(&peer))),
        );
        let contents = GroupBootstrapContents::try_from(payload)?;

        assert_eq!(&contents.group_id, created.group_id());
        assert_eq!(contents.pq_group_id, created.pq_group_id());
        assert_eq!(
            secret_bytes(&contents.group_state_ear_key),
            secret_bytes(created.group_state_ear_key())
        );
        assert_eq!(
            secret_bytes(&contents.identity_link_wrapper_key),
            secret_bytes(created.identity_link_wrapper_key())
        );
        let Some(BootstrapConnection::Accept {
            user_id,
            friendship_package,
            connection_offer_hash,
        }) = contents.connection
        else {
            panic!("expected an accept context");
        };
        assert_eq!(user_id, peer);
        assert_eq!(
            friendship_package.friendship_token,
            FriendshipToken::from_bytes(vec![1; 32])
        );
        assert_eq!(
            secret_bytes(&friendship_package.wai_ear_key),
            vec![2; AEAD_KEY_SIZE]
        );
        assert_eq!(
            connection_offer_hash,
            Some(ConnectionOfferHash::from_bytes([4u8; 32]))
        );

        txn.commit().await?;
        Ok(())
    }

    fn minimal_payload() -> GroupBootstrap {
        GroupBootstrap {
            group_id: Some(b"t-group-id".to_vec()),
            pq_group_id: None,
            group_state_ear_key: Some(vec![1; AEAD_KEY_SIZE]),
            identity_link_wrapper_key: Some(vec![2; AEAD_KEY_SIZE]),
            connection: None,
        }
    }

    #[test]
    fn contents_reject_absent_or_malformed_fields() {
        assert!(GroupBootstrapContents::try_from(minimal_payload()).is_ok());

        let payload = GroupBootstrap {
            group_id: None,
            ..minimal_payload()
        };
        assert!(GroupBootstrapContents::try_from(payload).is_err());

        let payload = GroupBootstrap {
            group_state_ear_key: None,
            ..minimal_payload()
        };
        assert!(GroupBootstrapContents::try_from(payload).is_err());

        let payload = GroupBootstrap {
            identity_link_wrapper_key: Some(vec![2; AEAD_KEY_SIZE - 1]),
            ..minimal_payload()
        };
        assert!(GroupBootstrapContents::try_from(payload).is_err());
    }

    #[test]
    fn contents_reject_incomplete_connection_contexts() {
        let with_connection = |connection| {
            GroupBootstrapContents::try_from(GroupBootstrap {
                connection: Some(connection),
                ..minimal_payload()
            })
        };

        let peer = peer_user_id();
        assert!(
            with_connection(ConnectionContext::Accept(AcceptContext {
                user_id: None,
                ..accept_context(&peer)
            }))
            .is_err()
        );
        assert!(
            with_connection(ConnectionContext::Accept(AcceptContext {
                friendship_token: None,
                ..accept_context(&peer)
            }))
            .is_err()
        );
        // An absent uuid must not decode into the nil user.
        assert!(
            with_connection(ConnectionContext::Accept(AcceptContext {
                user_id: Some(PeerUserId {
                    uuid: None,
                    domain: Some("example.com".to_owned()),
                }),
                ..accept_context(&peer)
            }))
            .is_err()
        );
        assert!(
            with_connection(ConnectionContext::HandleInitiator(HandleInitiatorContext {
                username: None,
                friendship_package_ear_key: Some(vec![5; AEAD_KEY_SIZE]),
                connection_offer_hash: Some(ConnectionOfferHash::from_bytes([6u8; 32])),
            }))
            .is_err()
        );
        // A context kind this version does not know cannot be installed.
        assert!(with_connection(ConnectionContext::Unknown).is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn seal_param_describes_the_created_group() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let self_group = SelfGroup::new_for_test(create_group(&mut txn, true)?);
        let created = create_group_with_vc_emulation(&mut txn, false, Some(self_group.group_id()))?;
        assert!(created.own_leaf_is_virtual_client());

        let param = self_group.seal_group_bootstrap_param(
            &mut txn,
            &created,
            GroupBootstrapCarrier::CreationEcho,
            None,
        )?;
        let blob: GroupBootstrapBlob = PersistenceCodec::from_slice(&param)?;
        assert!(blob.secret_info().is_some());
        assert!(blob.encrypted_bootstrap.is_some());

        let payload = group_bootstrap_payload(&created, None);
        assert_eq!(
            payload.group_id.as_deref(),
            Some(created.group_id().as_slice())
        );
        assert_eq!(
            payload.pq_group_id,
            created.pq_group_id().map(|id| id.as_slice().to_vec())
        );
        assert_eq!(
            payload.group_state_ear_key,
            Some(secret_bytes(created.group_state_ear_key()))
        );
        assert_eq!(
            payload.identity_link_wrapper_key,
            Some(secret_bytes(created.identity_link_wrapper_key()))
        );
        assert_eq!(payload.connection, None);

        txn.commit().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn seal_produces_blob_with_fresh_coordinates() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let group = create_group(&mut txn, true)?;
        let bootstrap = sample_bootstrap(&carried_group_id());

        let blob = group.seal_group_bootstrap(
            &mut txn,
            &bootstrap,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        )?;
        let info = blob.secret_info().expect("coordinates must be present");
        assert_eq!(info.leaf_index, group.mls_group().own_leaf_index());
        assert_eq!(info.generation, 0);
        assert!(blob.encrypted_bootstrap.is_some());

        // A second seal consumes the next generation.
        let blob = group.seal_group_bootstrap(
            &mut txn,
            &bootstrap,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        )?;
        assert_eq!(blob.secret_info().unwrap().generation, 1);

        txn.commit().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn seal_requires_self_group() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let group = create_group(&mut txn, false)?;
        let bootstrap = sample_bootstrap(&carried_group_id());

        let result = group.seal_group_bootstrap(
            &mut txn,
            &bootstrap,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        );
        assert!(result.is_err());

        txn.commit().await?;
        Ok(())
    }

    /// A blob names its sender's ratchet, so the sender itself must not be
    /// able to open it: rederiving from the own leaf would burn the ratchet
    /// head.
    #[tokio::test(flavor = "multi_thread")]
    async fn open_rejects_own_blob() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let group = create_group(&mut txn, true)?;
        let bootstrap = sample_bootstrap(&carried_group_id());

        let blob = group.seal_group_bootstrap(
            &mut txn,
            &bootstrap,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        )?;
        let result = group.open_group_bootstrap(
            &mut txn,
            &blob,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        );
        assert!(result.is_err());

        txn.commit().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_requires_coordinates_and_ciphertext() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let group = create_group(&mut txn, true)?;
        let blob = GroupBootstrapBlob {
            epoch_id: Some(b"emulation-epoch-id".to_vec()),
            sender_leaf_index: None,
            generation: Some(0),
            encrypted_bootstrap: None,
        };
        let result = group.open_group_bootstrap(
            &mut txn,
            &blob,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        );
        assert!(result.is_err());

        txn.commit().await?;
        Ok(())
    }
}
