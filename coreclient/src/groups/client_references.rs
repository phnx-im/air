// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The QS client references in the leaves of the self group.
//!
//! A commit to the self group that changes them contains our signature over
//! the references after the commit. The DS forwards the signature to the QS.
//! The QS then clears the push tokens of our client records that no leaf
//! names.

use std::collections::HashSet;

use aircommon::{
    identifiers::QsReference,
    messages::client_references::{
        ClientReferencesChange, SealedClientReferencesSignature, SelfGroupReferences,
    },
};
use anyhow::Context;
use openmls::prelude::{MlsGroup, StagedCommit};
use openmls_rust_crypto::RustCrypto;

use crate::{groups::Group, key_stores::MemoryUserKeyStore};

fn pending_change(
    mls_group: &MlsGroup,
    staged_commit: &StagedCommit,
) -> anyhow::Result<ClientReferencesChange> {
    let tree = mls_group.export_ratchet_tree();
    let before = SelfGroupReferences::from_leaves(tree.leaves());
    let tree = staged_commit
        .export_ratchet_tree(&RustCrypto::default(), tree)?
        .context("pending commit without a ratchet tree")?;
    let after = SelfGroupReferences::from_leaves(tree.leaves());
    Ok(ClientReferencesChange::between(before, after))
}

fn references_to_sign(
    change: ClientReferencesChange,
    sign_unchanged: bool,
) -> Option<HashSet<QsReference>> {
    match change {
        ClientReferencesChange::Unchanged(_) if !sign_unchanged => None,
        change => change.into_signed_references(),
    }
}

impl Group {
    /// The client references to sign for the pending commit of the self group.
    ///
    /// These are the references of all leaves after the commit. Returns `None`
    /// in these cases:
    /// - The group is not the self group.
    /// - A leaf will have no reference.
    /// - The commit leaves the references unchanged and `sign_unchanged` is
    ///   false.
    pub(crate) fn client_references_to_sign(
        &self,
        sign_unchanged: bool,
    ) -> anyhow::Result<Option<HashSet<QsReference>>> {
        if !self.is_self_group() {
            return Ok(None);
        }
        let mls_group = self.mls_group();
        let staged_commit = mls_group
            .pending_commit()
            .context("self-group operation without a pending commit")?;
        let change = pending_change(mls_group, staged_commit)?;
        Ok(references_to_sign(change, sign_unchanged))
    }

    /// Signs the references of [`Self::client_references_to_sign`] for the DS
    /// to forward to the QS.
    pub(crate) fn client_references_signature(
        &self,
        key_store: &MemoryUserKeyStore,
        sign_unchanged: bool,
    ) -> anyhow::Result<Option<SealedClientReferencesSignature>> {
        let Some(client_references) = self.client_references_to_sign(sign_unchanged)? else {
            return Ok(None);
        };
        Ok(Some(
            key_store.seal_client_references_signature(client_references)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::random_client_reference;

    use super::{SelfGroupReferences::Incomplete, *};

    fn set(client_references: &[&QsReference]) -> HashSet<QsReference> {
        let mut set = HashSet::new();
        for client_reference in client_references {
            set.insert((*client_reference).clone());
        }
        set
    }

    fn complete(client_references: &[&QsReference]) -> SelfGroupReferences {
        SelfGroupReferences::Complete(set(client_references))
    }

    fn change(before: SelfGroupReferences, after: SelfGroupReferences) -> ClientReferencesChange {
        ClientReferencesChange::between(before, after)
    }

    #[test]
    fn signs_changed_references() {
        let a = random_client_reference();
        let b = random_client_reference();

        let added = change(complete(&[&a]), complete(&[&a, &b]));
        assert_eq!(references_to_sign(added, false), Some(set(&[&a, &b])));
        let repaired = change(Incomplete, complete(&[&a, &b]));
        assert_eq!(references_to_sign(repaired, false), Some(set(&[&a, &b])));
    }

    #[test]
    fn signs_unchanged_references_on_request() {
        let a = random_client_reference();
        let unchanged = || change(complete(&[&a]), complete(&[&a]));

        assert_eq!(references_to_sign(unchanged(), false), None);
        assert_eq!(references_to_sign(unchanged(), true), Some(set(&[&a])));
    }

    #[test]
    fn signs_nothing_while_a_leaf_has_no_reference() {
        let a = random_client_reference();
        let dropped = change(complete(&[&a]), Incomplete);

        assert_eq!(references_to_sign(dropped, false), None);
        assert_eq!(
            references_to_sign(change(Incomplete, Incomplete), true),
            None
        );
    }
}
