// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Compatibility checks for add candidates.
//!
//! Mirrors the OpenMLS capabilities validation that runs when an add commit
//! is built and when the DS processes it. Checking candidates upfront allows
//! leaving incompatible ones out of the commit instead of failing the whole
//! commit.

use anyhow::Context;
use displaydoc::Display;
use openmls::prelude::{
    Capabilities, Ciphersuite, CredentialType, ExtensionType, KeyPackage, MlsGroup, ProposalType,
    ProtocolVersion, RequiredCapabilitiesExtension,
};

use crate::contacts::ContactKeyPackage;

use super::Group;

/// Reasons an add candidate is not compatible with a group.
#[derive(Debug, PartialEq, Eq, Display, thiserror::Error)]
pub(crate) enum InviteeIncompatibility {
    /// key package ciphersuite does not match the group
    Ciphersuite,
    /// capabilities do not cover the group ciphersuite or protocol version
    CiphersuiteOrVersionNotInCapabilities,
    /// leaf node does not support all group context extensions
    GroupContextExtensions,
    /// leaf node does not support all required extensions
    RequiredExtensions,
    /// leaf node does not support all required proposals
    RequiredProposals,
    /// leaf node does not support all required credentials
    RequiredCredentials,
    /// credential types are not mutually supported with existing members
    MemberCredentials,
}

impl Group {
    /// Checks whether an invitee's key package is compatible with this group.
    ///
    /// Compatible means that an add commit with this key package passes the
    /// OpenMLS capabilities validation, both locally when the commit is built
    /// and on the DS when it is processed.
    pub(crate) fn check_invitee_compatibility(
        &self,
        key_package: &ContactKeyPackage,
    ) -> anyhow::Result<Result<(), InviteeIncompatibility>> {
        match key_package {
            ContactKeyPackage::Traditional(key_package) => {
                Ok(check_key_package(&self.mls_group, key_package))
            }
            ContactKeyPackage::Apq(key_package) => {
                let pq = self.pq.as_ref().context("No PQ group found")?;
                let t_result = check_key_package(&self.mls_group, key_package.t_key_package());
                if t_result.is_err() {
                    return Ok(t_result);
                }
                Ok(check_key_package(
                    &pq.mls_group,
                    key_package.pq_key_package(),
                ))
            }
        }
    }
}

fn check_key_package(
    group: &MlsGroup,
    key_package: &KeyPackage,
) -> Result<(), InviteeIncompatibility> {
    let public_group = group.public_group();
    if key_package.ciphersuite() != public_group.ciphersuite() {
        return Err(InviteeIncompatibility::Ciphersuite);
    }
    let leaf_node = key_package.leaf_node();
    check_add_candidate_capabilities(
        public_group.ciphersuite(),
        public_group.version(),
        public_group
            .group_context()
            .extensions()
            .iter()
            .map(|extension| extension.extension_type()),
        public_group.required_capabilities(),
        public_group
            .treesync()
            .full_leaves()
            .map(|(_, leaf)| (leaf.credential().credential_type(), leaf.capabilities())),
        leaf_node.credential().credential_type(),
        leaf_node.capabilities(),
    )
}

/// Checks whether a leaf node with the given credential type and capabilities
/// can be added to a group with the given parameters.
///
/// Mirrors `PublicGroup::validate_add_proposals` and
/// `PublicGroup::validate_leaf_node_capabilities` in OpenMLS, which are
/// crate-private. See also `to_capabilities_mismatch`.
fn check_add_candidate_capabilities<'a>(
    ciphersuite: Ciphersuite,
    version: ProtocolVersion,
    group_context_extension_types: impl IntoIterator<Item = ExtensionType>,
    required_capabilities: Option<&RequiredCapabilitiesExtension>,
    members: impl IntoIterator<Item = (CredentialType, &'a Capabilities)>,
    candidate_credential_type: CredentialType,
    candidate: &Capabilities,
) -> Result<(), InviteeIncompatibility> {
    if !candidate.ciphersuites().contains(&ciphersuite.into())
        || !candidate.versions().contains(&version)
    {
        return Err(InviteeIncompatibility::CiphersuiteOrVersionNotInCapabilities);
    }

    let supports_extension = |extension_type: ExtensionType| {
        is_default_extension_type(extension_type)
            || candidate.extensions().contains(&extension_type)
    };

    for extension_type in group_context_extension_types {
        if !supports_extension(extension_type) {
            return Err(InviteeIncompatibility::GroupContextExtensions);
        }
    }

    if let Some(required_capabilities) = required_capabilities {
        if !required_capabilities
            .extension_types()
            .iter()
            .all(|&extension_type| supports_extension(extension_type))
        {
            return Err(InviteeIncompatibility::RequiredExtensions);
        }
        if !required_capabilities
            .proposal_types()
            .iter()
            .all(|&proposal_type| {
                is_default_proposal_type(proposal_type)
                    || candidate.proposals().contains(&proposal_type)
            })
        {
            return Err(InviteeIncompatibility::RequiredProposals);
        }
        if !required_capabilities
            .credential_types()
            .iter()
            .all(|credential_type| candidate.credentials().contains(credential_type))
        {
            return Err(InviteeIncompatibility::RequiredCredentials);
        }
    }

    for (member_credential_type, member_capabilities) in members {
        if !member_capabilities
            .credentials()
            .contains(&candidate_credential_type)
            || !candidate.credentials().contains(&member_credential_type)
        {
            return Err(InviteeIncompatibility::MemberCredentials);
        }
    }

    Ok(())
}

/// Extension types that RFC 9420 considers default and that are supported
/// without being listed in the capabilities. Mirrors the crate-private
/// `ExtensionType::is_default` in OpenMLS.
fn is_default_extension_type(extension_type: ExtensionType) -> bool {
    match extension_type {
        ExtensionType::ApplicationId
        | ExtensionType::RatchetTree
        | ExtensionType::RequiredCapabilities
        | ExtensionType::ExternalPub
        | ExtensionType::ExternalSenders => true,
        ExtensionType::LastResort
        | ExtensionType::AppDataDictionary
        | ExtensionType::Grease(_)
        | ExtensionType::Unknown(_) => false,
    }
}

/// Proposal types that RFC 9420 considers default and that are supported
/// without being listed in the capabilities. Mirrors the crate-private
/// `ProposalType::is_default` in OpenMLS.
fn is_default_proposal_type(proposal_type: ProposalType) -> bool {
    match proposal_type {
        ProposalType::Add
        | ProposalType::Update
        | ProposalType::Remove
        | ProposalType::PreSharedKey
        | ProposalType::Reinit
        | ProposalType::ExternalInit
        | ProposalType::GroupContextExtensions => true,
        ProposalType::SelfRemove
        | ProposalType::AppEphemeral
        | ProposalType::AppDataUpdate
        | ProposalType::Grease(_)
        | ProposalType::Custom(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use aircommon::mls_group_config::{
        GROUP_DATA_EXTENSION_TYPE, QS_CLIENT_REFERENCE_EXTENSION_TYPE, SUPPORTED_CIPHERSUITES,
        SUPPORTED_CREDENTIALS, SUPPORTED_EXTENSIONS, SUPPORTED_PROPOSALS,
        SUPPORTED_PROTOCOL_VERSIONS, default_group_required_extensions,
        default_leaf_node_capabilities,
    };

    use super::*;

    const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

    fn group_context_extension_types() -> Vec<ExtensionType> {
        vec![
            ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE),
            ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE),
            ExtensionType::AppDataDictionary,
        ]
    }

    fn check(candidate: &Capabilities) -> Result<(), InviteeIncompatibility> {
        let member_capabilities = default_leaf_node_capabilities();
        check_add_candidate_capabilities(
            CIPHERSUITE,
            ProtocolVersion::Mls10,
            group_context_extension_types(),
            Some(&default_group_required_extensions()),
            [(CredentialType::Basic, &member_capabilities)],
            CredentialType::Basic,
            candidate,
        )
    }

    #[test]
    fn default_capabilities_are_compatible() {
        assert_eq!(check(&default_leaf_node_capabilities()), Ok(()));
    }

    #[test]
    fn empty_capabilities_are_incompatible() {
        assert_eq!(
            check(&Capabilities::empty()),
            Err(InviteeIncompatibility::CiphersuiteOrVersionNotInCapabilities)
        );
    }

    #[test]
    fn missing_group_context_extension() {
        let candidate = Capabilities::new(
            Some(SUPPORTED_PROTOCOL_VERSIONS),
            Some(SUPPORTED_CIPHERSUITES),
            // An older client that does not know the app data dictionary.
            Some(&[
                ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE),
                ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE),
                ExtensionType::LastResort,
            ]),
            Some(SUPPORTED_PROPOSALS),
            Some(SUPPORTED_CREDENTIALS),
        );
        assert_eq!(
            check(&candidate),
            Err(InviteeIncompatibility::GroupContextExtensions)
        );
    }

    #[test]
    fn missing_required_extension() {
        let candidate = Capabilities::new(
            Some(SUPPORTED_PROTOCOL_VERSIONS),
            Some(SUPPORTED_CIPHERSUITES),
            Some(&[
                ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE),
                ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE),
                ExtensionType::AppDataDictionary,
            ]),
            Some(SUPPORTED_PROPOSALS),
            Some(SUPPORTED_CREDENTIALS),
        );
        assert_eq!(
            check(&candidate),
            Err(InviteeIncompatibility::RequiredExtensions)
        );
    }

    #[test]
    fn missing_required_proposal() {
        let candidate = Capabilities::new(
            Some(SUPPORTED_PROTOCOL_VERSIONS),
            Some(SUPPORTED_CIPHERSUITES),
            Some(SUPPORTED_EXTENSIONS),
            Some(&[ProposalType::SelfRemove]),
            Some(SUPPORTED_CREDENTIALS),
        );
        assert_eq!(
            check(&candidate),
            Err(InviteeIncompatibility::RequiredProposals)
        );
    }

    #[test]
    fn member_credential_type_not_supported() {
        let candidate = default_leaf_node_capabilities();
        let member_capabilities = default_leaf_node_capabilities();
        let result = check_add_candidate_capabilities(
            CIPHERSUITE,
            ProtocolVersion::Mls10,
            group_context_extension_types(),
            None,
            [(CredentialType::X509, &member_capabilities)],
            CredentialType::Basic,
            &candidate,
        );
        assert_eq!(result, Err(InviteeIncompatibility::MemberCredentials));
    }
}
