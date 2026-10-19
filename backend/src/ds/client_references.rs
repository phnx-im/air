// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rules for the QS client references in the leaves of a self group.
//!
//! The leaves of a user's self group contain QS client references. The QS
//! uses them to learn which client records of the user belong to a device. A
//! commit that changes the references must contain a signature by the user. We
//! forward the signature to the QS.

use aircommon::{
    identifiers::Fqdn,
    messages::client_references::SealedClientReferencesSignature,
    mls_group_config::{EncodedClientReference, leaf_client_reference},
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
    #[error("Client references signature while a self-group leaf has no client reference")]
    IncompleteReferences,
    #[error("Invalid client reference in a self-group leaf")]
    InvalidReference,
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
            Self::SelfGroup(SelfGroupReferences::from_group(group_state.group()))
        } else {
            Self::OtherGroup
        }
    }

    pub(super) fn compare(self, after: &DsGroupState) -> ClientReferencesChange {
        match self {
            Self::OtherGroup => ClientReferencesChange::OtherGroup,
            Self::SelfGroup(before) => {
                let after = SelfGroupReferences::from_group(after.group());
                ClientReferencesChange::between(before, after)
            }
        }
    }
}

/// The client references of all leaves of a self group. We sort them to make
/// them independent of the order of the leaves.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SelfGroupReferences {
    /// At least one leaf has no client reference.
    Incomplete(Vec<Option<EncodedClientReference>>),
    Complete(Vec<EncodedClientReference>),
}

impl SelfGroupReferences {
    fn from_group(group: &Group) -> Self {
        let slots = group
            .members()
            .map(|member| group.leaf(member.index).and_then(leaf_client_reference))
            .collect();
        Self::from_slots(slots)
    }

    fn from_slots(mut slots: Vec<Option<EncodedClientReference>>) -> Self {
        slots.sort();
        if slots.iter().all(Option::is_some) {
            Self::Complete(slots.into_iter().flatten().collect())
        } else {
            Self::Incomplete(slots)
        }
    }

    fn into_reconciliation(
        self,
        signature: SealedClientReferencesSignature,
        own_domain: &Fqdn,
    ) -> Result<ClientReferencesReconciliation, ClientReferencesError> {
        let encoded_references = match self {
            Self::Incomplete(_) => return Err(ClientReferencesError::IncompleteReferences),
            Self::Complete(encoded_references) => encoded_references,
        };
        let mut client_references = Vec::with_capacity(encoded_references.len());
        for encoded in encoded_references {
            let client_reference = encoded
                .decode()
                .map_err(|_| ClientReferencesError::InvalidReference)?;
            if &client_reference.client_homeserver_domain != own_domain {
                return Err(ClientReferencesError::ForeignReference);
            }
            client_references.push(client_reference);
        }
        Ok(ClientReferencesReconciliation {
            client_references,
            signature,
        })
    }
}

/// What a commit or resync did to the client references of a group.
pub(super) enum ClientReferencesChange {
    OtherGroup,
    Unchanged(SelfGroupReferences),
    Changed {
        before: SelfGroupReferences,
        after: SelfGroupReferences,
    },
}

impl ClientReferencesChange {
    fn between(before: SelfGroupReferences, after: SelfGroupReferences) -> Self {
        if before == after {
            Self::Unchanged(after)
        } else {
            Self::Changed { before, after }
        }
    }

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

    /// A commit that changes the client references of a self group requires a
    /// signature over the references after the commit. Other commits to a self
    /// group may contain one.
    fn reconciliation(
        self,
        signature: Option<SealedClientReferencesSignature>,
        own_domain: &Fqdn,
    ) -> Result<Option<ClientReferencesReconciliation>, ClientReferencesError> {
        use SelfGroupReferences::{Complete, Incomplete};
        match (self, signature) {
            (Self::OtherGroup, None) => Ok(None),
            (Self::OtherGroup, Some(_)) => Err(ClientReferencesError::UnexpectedSignature),
            (Self::Unchanged(_), None) => Ok(None),
            (
                Self::Changed {
                    before: Incomplete(_),
                    after: _,
                },
                None,
            ) => Ok(None),
            (
                Self::Changed {
                    before: Complete(_),
                    after: _,
                },
                None,
            ) => Err(ClientReferencesError::MissingSignature),
            (Self::Unchanged(after) | Self::Changed { before: _, after }, Some(signature)) => {
                after.into_reconciliation(signature, own_domain).map(Some)
            }
        }
    }

    /// A resync contains no signature. It must leave the client references as
    /// they are.
    pub(super) fn check_resync(&self) -> Result<(), ClientReferencesError> {
        match self {
            Self::OtherGroup | Self::Unchanged(_) => Ok(()),
            Self::Changed {
                before: _,
                after: _,
            } => Err(ClientReferencesError::ResyncChangesReferences),
        }
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        crypto::hpke::ClientIdDecryptionKey,
        identifiers::{QsClientId, QsReference},
        messages::client_references::SealedClientReferencesSignature,
        virtual_client::KeyPackageBatchId,
    };
    use mls_assist::{
        openmls::{components::vc_derivation_info::EpochId, prelude::LeafNodeIndex},
        openmls_traits::types::HpkeCiphertext,
    };
    use tls_codec::Serialize;

    use super::{ClientReferencesError::*, *};

    fn own_domain() -> Fqdn {
        "example.com".parse().unwrap()
    }

    fn encoded_reference(domain: Fqdn) -> EncodedClientReference {
        let decryption_key = ClientIdDecryptionKey::generate().unwrap();
        let sealed_reference = decryption_key
            .encryption_key()
            .seal_client_config(aircommon::identifiers::ClientConfig {
                client_id: QsClientId::random(&mut rand::rng()),
                push_token_ear_key: None,
            })
            .unwrap();
        QsReference {
            client_homeserver_domain: domain,
            sealed_reference,
        }
        .tls_serialize_detached()
        .unwrap()
        .into()
    }

    fn change(
        before: &[Option<&EncodedClientReference>],
        after: &[Option<&EncodedClientReference>],
    ) -> ClientReferencesChange {
        let references = |slots: &[Option<&EncodedClientReference>]| {
            SelfGroupReferences::from_slots(slots.iter().map(|slot| slot.cloned()).collect())
        };
        ClientReferencesChange::between(references(before), references(after))
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
        change: ClientReferencesChange,
        signature: Option<SealedClientReferencesSignature>,
    ) -> Result<Option<usize>, ClientReferencesError> {
        let reconciliation = change.reconciliation(signature, &own_domain())?;
        Ok(reconciliation.map(|reconciliation| reconciliation.client_references.len()))
    }

    #[test]
    fn changed_references_require_a_signature() {
        let a = encoded_reference(own_domain());
        let b = encoded_reference(own_domain());
        let before = vec![Some(&a)];
        let added = vec![Some(&a), Some(&b)];
        let removed = vec![Some(&b)];

        assert_eq!(
            reconcile(change(&before, &added), None),
            Err(MissingSignature)
        );
        assert_eq!(
            reconcile(change(&added, &removed), None),
            Err(MissingSignature)
        );
        assert_eq!(reconcile(change(&before, &added), signature()), Ok(Some(2)));
        assert_eq!(
            reconcile(change(&added, &removed), signature()),
            Ok(Some(1))
        );
    }

    #[test]
    fn unchanged_references_take_an_optional_signature() {
        let a = encoded_reference(own_domain());
        let b = encoded_reference(own_domain());
        let before = vec![Some(&a), Some(&b)];
        // A resync may move a leaf. Moving a leaf does not change the references.
        let moved = vec![Some(&b), Some(&a)];

        assert_eq!(reconcile(change(&before, &before), None), Ok(None));
        assert_eq!(reconcile(change(&before, &moved), None), Ok(None));
        assert_eq!(
            reconcile(change(&before, &before), signature()),
            Ok(Some(2))
        );
    }

    #[test]
    fn complete_references_stay_complete() {
        let a = encoded_reference(own_domain());
        let b = encoded_reference(own_domain());
        let before = vec![Some(&a), Some(&b)];
        let dropped = vec![Some(&a), None];

        assert_eq!(
            reconcile(change(&before, &dropped), None),
            Err(MissingSignature)
        );
        assert_eq!(
            reconcile(change(&before, &dropped), signature()),
            Err(IncompleteReferences)
        );
    }

    #[test]
    fn incomplete_references_are_not_checked() {
        let a = encoded_reference(own_domain());
        let b = encoded_reference(own_domain());
        let c = encoded_reference(own_domain());
        let before = vec![None, None, Some(&c)];
        let repaired = vec![Some(&a), None, Some(&c)];
        let added = vec![None, None, Some(&c), Some(&b)];
        let removed = vec![None, None];
        let completed = vec![Some(&a), Some(&b), Some(&c)];

        for after in [&before, &repaired, &added, &removed, &completed] {
            assert_eq!(reconcile(change(&before, after), None), Ok(None));
        }
        assert_eq!(
            reconcile(change(&before, &completed), signature()),
            Ok(Some(3))
        );
        assert_eq!(
            reconcile(change(&before, &repaired), signature()),
            Err(IncompleteReferences)
        );
    }

    #[test]
    fn other_groups_take_no_signature() {
        assert!(matches!(
            ClientReferencesChange::OtherGroup.commit_hint(None, key_package_hint(), &own_domain()),
            Ok(Some(QsVirtualClientHint::PromoteStagedKeyPackages(_)))
        ));
        assert!(matches!(
            ClientReferencesChange::OtherGroup.commit_hint(signature(), None, &own_domain()),
            Err(UnexpectedSignature)
        ));
    }

    #[test]
    fn signature_excludes_key_package_upload() {
        let a = encoded_reference(own_domain());
        let before = vec![Some(&a)];
        let commit_hint = |signature, key_package_hint| {
            change(&before, &before).commit_hint(signature, key_package_hint, &own_domain())
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
        let a = encoded_reference(own_domain());
        let b = encoded_reference(own_domain());
        let before = vec![Some(&a), Some(&b)];

        assert_eq!(ClientReferencesChange::OtherGroup.check_resync(), Ok(()));
        let moved = vec![Some(&b), Some(&a)];
        assert_eq!(change(&before, &moved).check_resync(), Ok(()));
        let dropped = vec![Some(&a), None];
        assert_eq!(
            change(&before, &dropped).check_resync(),
            Err(ResyncChangesReferences)
        );
        let removed = vec![Some(&b)];
        assert_eq!(
            change(&before, &removed).check_resync(),
            Err(ResyncChangesReferences)
        );
        let incomplete = vec![None];
        assert_eq!(change(&incomplete, &incomplete).check_resync(), Ok(()));
    }

    #[test]
    fn rejects_invalid_and_foreign_references() {
        let a = encoded_reference(own_domain());
        let invalid = EncodedClientReference::from(vec![0xff]);
        let foreign = encoded_reference("other.example.com".parse().unwrap());
        let before = vec![Some(&a)];

        let after = vec![Some(&a), Some(&invalid)];
        assert_eq!(
            reconcile(change(&before, &after), signature()),
            Err(InvalidReference)
        );
        let after = vec![Some(&a), Some(&foreign)];
        assert_eq!(
            reconcile(change(&before, &after), signature()),
            Err(ForeignReference)
        );
    }
}
