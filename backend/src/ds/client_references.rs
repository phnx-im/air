// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rules for the QS client references in the leaves of a self group.
//!
//! The leaves of a user's self group contain QS client references. The QS
//! uses them to learn which client records of the user belong to a device.
//! [`ClientReferencesChange`] decides when a commit needs a signature by the
//! user. We forward the signature to the QS.

use aircommon::{
    identifiers::Fqdn,
    messages::client_references::{
        ClientReferencesChange, SealedClientReferencesSignature, SelfGroupReferences,
    },
};
use mls_assist::group::Group;
use thiserror::Error;

use crate::messages::intra_backend::{ClientReferencesReconciliation, QsVirtualClientHint};

use super::group_state::DsGroupState;

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum ClientReferencesError {
    #[error("Self-group commit changes the client references without a signature")]
    MissingSignature,
    #[error("Unexpected client references signature")]
    UnexpectedSignature,
    #[error("Client references signature while a self-group leaf has no valid client reference")]
    IncompleteReferences,
    #[error("Client reference of another domain in a self-group leaf")]
    ForeignReference,
    #[error("Commit carries both a key package upload and a client references signature")]
    ConflictingHints,
    #[error("Self-group resync changes the client references")]
    ResyncChangesReferences,
}

/// The client references in the leaves of a group before a commit or resync.
pub(super) enum ClientReferencesSnapshot {
    /// The QS only learns client references from self groups.
    OtherGroup,
    SelfGroup(SelfGroupReferences),
}

impl ClientReferencesSnapshot {
    pub(super) fn take(group_state: &DsGroupState) -> Self {
        if group_state.is_self_group() {
            Self::SelfGroup(self_group_references(group_state.group()))
        } else {
            Self::OtherGroup
        }
    }

    pub(super) fn compare(self, after: &DsGroupState) -> GroupReferencesChange {
        match self {
            Self::OtherGroup => GroupReferencesChange::OtherGroup,
            Self::SelfGroup(before) => {
                let after = self_group_references(after.group());
                GroupReferencesChange::SelfGroup(ClientReferencesChange::between(before, after))
            }
        }
    }
}

fn self_group_references(group: &Group) -> SelfGroupReferences {
    // Every member has a leaf.
    let leaves = group
        .members()
        .filter_map(|member| group.leaf(member.index));
    SelfGroupReferences::from_leaves(leaves)
}

/// What a commit or resync did to the client references of a group.
pub(super) enum GroupReferencesChange {
    OtherGroup,
    SelfGroup(ClientReferencesChange),
}

impl GroupReferencesChange {
    /// Checks the signature of a commit over the client references. Returns the
    /// hint for the QS. `key_package_hint` is the other hint a commit may
    /// contain.
    pub(super) fn commit_hint(
        self,
        signature: Option<SealedClientReferencesSignature>,
        key_package_hint: Option<QsVirtualClientHint>,
        own_domain: &Fqdn,
    ) -> Result<Option<QsVirtualClientHint>, ClientReferencesError> {
        let reconciliation = self.reconciliation(signature, own_domain)?;
        match (key_package_hint, reconciliation) {
            (Some(_), Some(_)) => Err(ClientReferencesError::ConflictingHints),
            (key_package_hint, None) => Ok(key_package_hint),
            (None, Some(reconciliation)) => {
                Ok(Some(QsVirtualClientHint::ReconcileClients(reconciliation)))
            }
        }
    }

    /// Applies the rule of [`ClientReferencesChange`]. Commits to other groups
    /// contain no signature.
    fn reconciliation(
        self,
        signature: Option<SealedClientReferencesSignature>,
        own_domain: &Fqdn,
    ) -> Result<Option<ClientReferencesReconciliation>, ClientReferencesError> {
        match (self, signature) {
            (Self::OtherGroup, None) => Ok(None),
            (Self::OtherGroup, Some(_)) => Err(ClientReferencesError::UnexpectedSignature),
            (Self::SelfGroup(change), None) => {
                if change.requires_signature() {
                    Err(ClientReferencesError::MissingSignature)
                } else {
                    Ok(None)
                }
            }
            (Self::SelfGroup(change), Some(signature)) => {
                let client_references = change
                    .into_signed_references()
                    .ok_or(ClientReferencesError::IncompleteReferences)?;
                for client_reference in &client_references {
                    if &client_reference.client_homeserver_domain != own_domain {
                        return Err(ClientReferencesError::ForeignReference);
                    }
                }
                Ok(Some(ClientReferencesReconciliation {
                    client_references: client_references.into_iter().collect(),
                    signature,
                }))
            }
        }
    }

    /// A resync contains no signature. It must leave the client references as
    /// they are.
    pub(super) fn check_resync(&self) -> Result<(), ClientReferencesError> {
        match self {
            Self::OtherGroup | Self::SelfGroup(ClientReferencesChange::Unchanged(_)) => Ok(()),
            Self::SelfGroup(ClientReferencesChange::Changed {
                before: _,
                after: _,
            }) => Err(ClientReferencesError::ResyncChangesReferences),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use aircommon::{
        crypto::hpke::ClientIdDecryptionKey,
        identifiers::{ClientConfig, QsClientId, QsReference},
        virtual_client::KeyPackageBatchId,
    };
    use mls_assist::{
        openmls::{components::vc_derivation_info::EpochId, prelude::LeafNodeIndex},
        openmls_traits::types::HpkeCiphertext,
    };

    use super::{ClientReferencesError::*, SelfGroupReferences::Incomplete, *};

    fn own_domain() -> Fqdn {
        "example.com".parse().unwrap()
    }

    fn client_reference(domain: Fqdn) -> QsReference {
        let decryption_key = ClientIdDecryptionKey::generate().unwrap();
        let sealed_reference = decryption_key
            .encryption_key()
            .seal_client_config(ClientConfig {
                client_id: QsClientId::random(&mut rand::rng()),
                push_token_ear_key: None,
            })
            .unwrap();
        QsReference {
            client_homeserver_domain: domain,
            sealed_reference,
        }
    }

    fn complete(client_references: &[&QsReference]) -> SelfGroupReferences {
        let mut set = HashSet::new();
        for client_reference in client_references {
            set.insert((*client_reference).clone());
        }
        SelfGroupReferences::Complete(set)
    }

    fn change(before: SelfGroupReferences, after: SelfGroupReferences) -> GroupReferencesChange {
        GroupReferencesChange::SelfGroup(ClientReferencesChange::between(before, after))
    }

    fn key_package_hint() -> Option<QsVirtualClientHint> {
        Some(QsVirtualClientHint::PromoteStagedKeyPackages(
            KeyPackageBatchId {
                epoch_id: EpochId::new(b"epoch".to_vec()),
                leaf_index: LeafNodeIndex::new(0),
                generation: 0,
            },
        ))
    }

    fn signature() -> Option<SealedClientReferencesSignature> {
        Some(SealedClientReferencesSignature::from(HpkeCiphertext {
            kem_output: vec![1].into(),
            ciphertext: vec![2].into(),
        }))
    }

    fn reconcile(
        change: GroupReferencesChange,
        signature: Option<SealedClientReferencesSignature>,
    ) -> Result<Option<usize>, ClientReferencesError> {
        let reconciliation = change.reconciliation(signature, &own_domain())?;
        Ok(reconciliation.map(|reconciliation| reconciliation.client_references.len()))
    }

    #[test]
    fn changed_references_require_a_signature() {
        let a = client_reference(own_domain());
        let b = client_reference(own_domain());
        let added = || change(complete(&[&a]), complete(&[&a, &b]));

        assert_eq!(reconcile(added(), None), Err(MissingSignature));
        assert_eq!(reconcile(added(), signature()), Ok(Some(2)));
    }

    #[test]
    fn unchanged_references_take_an_optional_signature() {
        let a = client_reference(own_domain());
        let b = client_reference(own_domain());
        let unchanged = || change(complete(&[&a, &b]), complete(&[&a, &b]));

        assert_eq!(reconcile(unchanged(), None), Ok(None));
        assert_eq!(reconcile(unchanged(), signature()), Ok(Some(2)));
    }

    #[test]
    fn complete_references_stay_complete() {
        let a = client_reference(own_domain());
        let b = client_reference(own_domain());
        let dropped = || change(complete(&[&a, &b]), Incomplete);

        assert_eq!(reconcile(dropped(), None), Err(MissingSignature));
        assert_eq!(reconcile(dropped(), signature()), Err(IncompleteReferences));
    }

    #[test]
    fn incomplete_references_are_not_checked() {
        let a = client_reference(own_domain());
        let completed = || change(Incomplete, complete(&[&a]));

        assert_eq!(reconcile(completed(), None), Ok(None));
        assert_eq!(reconcile(completed(), signature()), Ok(Some(1)));
        assert_eq!(reconcile(change(Incomplete, Incomplete), None), Ok(None));
        assert_eq!(
            reconcile(change(Incomplete, Incomplete), signature()),
            Err(IncompleteReferences)
        );
    }

    #[test]
    fn other_groups_take_no_signature() {
        assert!(matches!(
            GroupReferencesChange::OtherGroup.commit_hint(None, key_package_hint(), &own_domain()),
            Ok(Some(QsVirtualClientHint::PromoteStagedKeyPackages(_)))
        ));
        assert!(matches!(
            GroupReferencesChange::OtherGroup.commit_hint(signature(), None, &own_domain()),
            Err(UnexpectedSignature)
        ));
    }

    #[test]
    fn signature_excludes_key_package_upload() {
        let a = client_reference(own_domain());
        let commit_hint = |signature, key_package_hint| {
            change(complete(&[&a]), complete(&[&a])).commit_hint(
                signature,
                key_package_hint,
                &own_domain(),
            )
        };

        assert!(matches!(
            commit_hint(signature(), key_package_hint()),
            Err(ConflictingHints)
        ));
        assert!(matches!(
            commit_hint(None, key_package_hint()),
            Ok(Some(QsVirtualClientHint::PromoteStagedKeyPackages(_)))
        ));
        assert!(matches!(
            commit_hint(signature(), None),
            Ok(Some(QsVirtualClientHint::ReconcileClients(_)))
        ));
    }

    #[test]
    fn resync_keeps_the_references() {
        let a = client_reference(own_domain());
        let b = client_reference(own_domain());
        let before = || complete(&[&a, &b]);

        assert_eq!(GroupReferencesChange::OtherGroup.check_resync(), Ok(()));
        assert_eq!(change(before(), before()).check_resync(), Ok(()));
        assert_eq!(
            change(before(), Incomplete).check_resync(),
            Err(ResyncChangesReferences)
        );
        assert_eq!(
            change(before(), complete(&[&b])).check_resync(),
            Err(ResyncChangesReferences)
        );
        assert_eq!(change(Incomplete, Incomplete).check_resync(), Ok(()));
    }

    #[test]
    fn rejects_foreign_references() {
        let a = client_reference(own_domain());
        let foreign = client_reference("other.example.com".parse().unwrap());
        let added = change(complete(&[&a]), complete(&[&a, &foreign]));

        assert_eq!(reconcile(added, signature()), Err(ForeignReference));
    }
}
