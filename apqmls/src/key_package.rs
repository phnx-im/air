// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use openmls::{
    prelude::{
        Capabilities, Ciphersuite, Extensions, KeyPackage, KeyPackageBuilder, KeyPackageBundle,
        KeyPackageNewError as OpenMlsKeyPackageNewError, KeyPackageVerifyError, LeafNode, Lifetime,
        OpenMlsCrypto, ProtocolVersion,
    },
    storage::OpenMlsProvider,
};
use serde::{Deserialize, Serialize};
use tap::Pipe as _;
use thiserror::Error;

use crate::{
    ApqCiphersuite,
    authentication::{ApqCredentialWithKey, ApqSigner},
    extension::{ensure_extension_support, ensure_leaf_node_component_support},
    messages::{ApqKeyPackage, ApqKeyPackageIn},
};

/// Errors that can occur when creating a new [`ApqKeyPackage`].
#[derive(Error, Debug)]
pub enum KeyPackageNewError {
    #[error(transparent)]
    OpenMls(#[from] OpenMlsKeyPackageNewError),
    #[error("Unsupported ciphersuite")]
    UnsupportedCiphersuite(#[from] tls_codec::Error),
}

/// A builder for creating a new [`ApqKeyPackage`].
pub struct ApqKeyPackageBuilder {
    capabilities: Capabilities,
    t_kp_builder: KeyPackageBuilder,
    pq_kp_builder: KeyPackageBuilder,
    key_package_extensions: Extensions<KeyPackage>,
    leaf_node_extensions: Extensions<LeafNode>,
}

/// A bundle consisting of an [`ApqKeyPackage`] and its corresponding
/// private keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApqKeyPackageBundle {
    t_kp_bundle: KeyPackageBundle,
    pq_kp_bundle: KeyPackageBundle,
}

impl ApqKeyPackageBundle {
    pub fn into_key_package(self) -> ApqKeyPackage {
        ApqKeyPackage {
            t_key_package: self.t_kp_bundle.key_package().clone(),
            pq_key_package: self.pq_kp_bundle.key_package().clone(),
        }
    }
}

impl Default for ApqKeyPackageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ApqKeyPackageBuilder {
    /// Create a key package builder.
    pub fn new() -> Self {
        Self {
            capabilities: Capabilities::default(),
            t_kp_builder: KeyPackageBuilder::new(),
            pq_kp_builder: KeyPackageBuilder::new(),
            key_package_extensions: Extensions::default(),
            leaf_node_extensions: Extensions::default(),
        }
    }

    /// Set the key package lifetime.
    pub fn key_package_lifetime(mut self, lifetime: Lifetime) -> Self {
        self.t_kp_builder = self.t_kp_builder.key_package_lifetime(lifetime);
        self.pq_kp_builder = self.pq_kp_builder.key_package_lifetime(lifetime);
        self
    }

    /// Set the key package extensions.
    pub fn key_package_extensions(mut self, extensions: Extensions<KeyPackage>) -> Self {
        self.key_package_extensions = extensions;
        self
    }

    /// Mark the key package as a last-resort key package via a
    /// [`openmls::extensions::LastResortExtension`].
    pub fn mark_as_last_resort(mut self) -> Self {
        self.t_kp_builder = self.t_kp_builder.mark_as_last_resort();
        self.pq_kp_builder = self.pq_kp_builder.mark_as_last_resort();
        self
    }

    /// Set the leaf node capabilities.
    pub fn leaf_node_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        // The capabilities are set in `build`, so we don't set them here.
        self
    }

    /// Set the leaf node extensions.
    pub fn leaf_node_extensions(mut self, extensions: Extensions<LeafNode>) -> Self {
        self.leaf_node_extensions = extensions;
        self
    }

    /// Finalize and build the key package.
    pub fn build(
        mut self,
        provider: &impl OpenMlsProvider,
        ciphersuite: ApqCiphersuite,
        signer: &impl ApqSigner,
        credential_with_key: ApqCredentialWithKey,
    ) -> Result<ApqKeyPackageBundle, KeyPackageNewError> {
        let capabilities = self
            .capabilities
            .pipe(ensure_extension_support)?
            .pipe(|c| ensure_ciphersuite_support(c, ciphersuite))?;

        let ln_extensions = self
            .leaf_node_extensions
            .pipe(ensure_leaf_node_component_support)?;
        let pk_extensions = self.key_package_extensions;

        self.t_kp_builder = self
            .t_kp_builder
            .leaf_node_capabilities(capabilities.clone())
            .leaf_node_extensions(ln_extensions.clone())
            .key_package_extensions(pk_extensions.clone());
        self.pq_kp_builder = self
            .pq_kp_builder
            .leaf_node_capabilities(capabilities)
            .leaf_node_extensions(ln_extensions)
            .key_package_extensions(pk_extensions);
        let t_kp_bundle = self.t_kp_builder.build(
            ciphersuite.t_ciphersuite,
            provider,
            signer.t_signer(),
            credential_with_key.t_credential,
        )?;
        let pq_kp_bundle = self.pq_kp_builder.build(
            ciphersuite.pq_ciphersuite,
            provider,
            signer.pq_signer(),
            credential_with_key.pq_credential,
        )?;
        Ok(ApqKeyPackageBundle {
            t_kp_bundle,
            pq_kp_bundle,
        })
    }
}

impl ApqKeyPackage {
    pub fn builder() -> ApqKeyPackageBuilder {
        ApqKeyPackageBuilder::new()
    }
}

impl ApqKeyPackageIn {
    pub fn validate(
        self,
        crypto: &impl OpenMlsCrypto,
    ) -> Result<ApqKeyPackage, KeyPackageVerifyError> {
        let protocol_version = ProtocolVersion::default();
        let t_key_package = self.t_key_package.validate(crypto, protocol_version)?;
        let pq_key_package = self.pq_key_package.validate(crypto, protocol_version)?;
        Ok(ApqKeyPackage {
            t_key_package,
            pq_key_package,
        })
    }
}

pub(super) fn ensure_ciphersuite_support(
    capabilities: Capabilities,
    ciphersuite: ApqCiphersuite,
) -> Result<Capabilities, tls_codec::Error> {
    let mut ciphersuites: HashSet<Ciphersuite> = capabilities
        .ciphersuites()
        .iter()
        .map(|&cs| cs.try_into())
        .collect::<Result<_, _>>()?;
    ciphersuites.insert(ciphersuite.t_ciphersuite);
    ciphersuites.insert(ciphersuite.pq_ciphersuite);
    let ciphersuites: Vec<Ciphersuite> = ciphersuites.into_iter().collect();
    Capabilities::new(
        Some(capabilities.versions()),
        Some(&ciphersuites),
        Some(capabilities.extensions()),
        Some(capabilities.proposals()),
        Some(capabilities.credentials()),
    )
    .pipe(Ok)
}
