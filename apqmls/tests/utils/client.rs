// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::{
    ApqCiphersuite,
    authentication::{ApqCredentialWithKey, ApqSignatureKeyPair, ApqSignatureScheme},
    messages::ApqKeyPackage,
};
use openmls::storage::OpenMlsProvider;

pub struct Client<Provider> {
    pub signer: ApqSignatureKeyPair,
    pub credential_with_key: ApqCredentialWithKey,
    pub provider: Provider,
}

impl<Provider: OpenMlsProvider> Client<Provider> {
    pub fn new(identity: &str, signature_scheme: ApqSignatureScheme, provider: Provider) -> Self {
        let keypair = ApqSignatureKeyPair::new(signature_scheme).unwrap();
        let credential_with_key = ApqCredentialWithKey::new(identity.as_bytes(), &keypair);

        Client {
            signer: keypair,
            credential_with_key,
            provider,
        }
    }

    pub fn generate_key_package(&self, ciphersuite: ApqCiphersuite) -> ApqKeyPackage {
        ApqKeyPackage::builder()
            .build(
                &self.provider,
                ciphersuite,
                &self.signer,
                self.credential_with_key.clone(),
            )
            .unwrap()
            .into_key_package()
    }
}
