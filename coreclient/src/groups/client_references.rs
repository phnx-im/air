// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The QS client references in the leaves of the self group.
//!
//! A commit to the self group that changes them contains our signature over
//! the references after the commit. The DS forwards the signature to the QS.
//! The QS then clears the push tokens of our client records that no leaf
//! names.

use aircommon::{
    identifiers::QsReference,
    messages::client_references::SealedClientReferencesSignature,
    mls_group_config::{EncodedClientReference, leaf_client_reference},
};
use anyhow::Context;
use openmls::{
    prelude::{MlsGroup, StagedCommit},
    treesync::RatchetTree,
};
use openmls_rust_crypto::RustCrypto;

use crate::{groups::Group, key_stores::MemoryUserKeyStore};

/// The client references of all leaves before and after a commit, sorted.
#[derive(Debug, PartialEq, Eq)]
struct ClientReferencesChange {
    before: Vec<Option<EncodedClientReference>>,
    after: Vec<Option<EncodedClientReference>>,
}

impl ClientReferencesChange {
    fn of_pending_commit(
        mls_group: &MlsGroup,
        staged_commit: &StagedCommit,
    ) -> anyhow::Result<Self> {
        let tree = mls_group.export_ratchet_tree();
        let before = sorted_client_references(&tree);
        let tree = staged_commit
            .export_ratchet_tree(&RustCrypto::default(), tree)?
            .context("pending commit without a ratchet tree")?;
        let after = sorted_client_references(&tree);
        Ok(Self { before, after })
    }

    /// We sign nothing while a leaf has no reference. The DS has the same rule.
    fn references_to_sign(self, sign_unchanged: bool) -> Option<Vec<EncodedClientReference>> {
        if !sign_unchanged && self.before == self.after {
            return None;
        }
        self.after.into_iter().collect()
    }
}

fn sorted_client_references(tree: &RatchetTree) -> Vec<Option<EncodedClientReference>> {
    let mut client_references: Vec<_> = tree.leaves().map(leaf_client_reference).collect();
    client_references.sort();
    client_references
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
    ) -> anyhow::Result<Option<Vec<QsReference>>> {
        if !self.is_self_group() {
            return Ok(None);
        }
        let mls_group = self.mls_group();
        let staged_commit = mls_group
            .pending_commit()
            .context("self-group operation without a pending commit")?;
        let change = ClientReferencesChange::of_pending_commit(mls_group, staged_commit)?;
        let Some(encoded) = change.references_to_sign(sign_unchanged) else {
            return Ok(None);
        };
        let mut client_references = Vec::with_capacity(encoded.len());
        for encoded in encoded {
            client_references.push(encoded.decode()?);
        }
        Ok(Some(client_references))
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
    use super::*;

    fn encoded(byte: u8) -> EncodedClientReference {
        vec![byte].into()
    }

    fn change(before: &[Option<u8>], after: &[Option<u8>]) -> ClientReferencesChange {
        let encode = |slots: &[Option<u8>]| {
            let mut slots: Vec<_> = slots.iter().map(|slot| slot.map(encoded)).collect();
            slots.sort();
            slots
        };
        ClientReferencesChange {
            before: encode(before),
            after: encode(after),
        }
    }

    #[test]
    fn signs_changed_references() {
        let added = change(&[Some(1)], &[Some(1), Some(2)]);
        assert_eq!(
            added.references_to_sign(false),
            Some(vec![encoded(1), encoded(2)])
        );
        let removed = change(&[Some(1), Some(2)], &[Some(2)]);
        assert_eq!(removed.references_to_sign(false), Some(vec![encoded(2)]));
        let repaired = change(&[None, Some(2)], &[Some(1), Some(2)]);
        assert_eq!(
            repaired.references_to_sign(false),
            Some(vec![encoded(1), encoded(2)])
        );
    }

    #[test]
    fn signs_unchanged_references_on_request() {
        let unchanged = change(&[Some(1), Some(2)], &[Some(2), Some(1)]);
        assert_eq!(unchanged.references_to_sign(false), None);
        let unchanged = change(&[Some(1), Some(2)], &[Some(2), Some(1)]);
        assert_eq!(
            unchanged.references_to_sign(true),
            Some(vec![encoded(1), encoded(2)])
        );
    }

    #[test]
    fn signs_nothing_while_a_leaf_has_no_reference() {
        let incomplete = change(&[None], &[None, Some(2)]);
        assert_eq!(incomplete.references_to_sign(false), None);
        let incomplete = change(&[None], &[None]);
        assert_eq!(incomplete.references_to_sign(true), None);
    }
}
