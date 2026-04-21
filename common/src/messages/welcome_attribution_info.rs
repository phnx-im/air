// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mls_assist::openmls::prelude::GroupId;
use tls_codec::Serialize;

use crate::{
    credentials::keys::{ClientKeyType, ClientSignature},
    crypto::{
        aead::{
            AeadDecryptable, AeadEncryptable,
            keys::{IdentityLinkWrapperKey, WelcomeAttributionInfoEarKey},
        },
        signatures::signable::{Signable, SignedStruct, Verifiable, VerifiedStruct},
    },
    identifiers::UserId,
};

use super::*;

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize, Serialize, Deserialize)]
pub struct WelcomeAttributionInfoPayload {
    sender_user_id: UserId,
    identity_link_wrapper_key: IdentityLinkWrapperKey,
}

impl WelcomeAttributionInfoPayload {
    pub fn new(sender_user_id: UserId, identity_link_wrapper_key: IdentityLinkWrapperKey) -> Self {
        Self {
            sender_user_id,
            identity_link_wrapper_key,
        }
    }

    pub fn identity_link_wrapper_key(&self) -> &IdentityLinkWrapperKey {
        &self.identity_link_wrapper_key
    }
}

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct WelcomeAttributionInfoTbs {
    pub payload: WelcomeAttributionInfoPayload,
    pub group_id: GroupId,
    pub welcome: Vec<u8>,
}

impl Signable for WelcomeAttributionInfoTbs {
    type SignedOutput = WelcomeAttributionInfo;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.tls_serialize_detached()
    }

    fn label(&self) -> &str {
        "WelcomeAttributionInfo"
    }
}

impl SignedStruct<WelcomeAttributionInfoTbs, ClientKeyType> for WelcomeAttributionInfo {
    fn from_payload(payload: WelcomeAttributionInfoTbs, signature: ClientSignature) -> Self {
        Self {
            payload: payload.payload,
            signature,
        }
    }
}

#[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize, Serialize, Deserialize)]
pub struct WelcomeAttributionInfo {
    payload: WelcomeAttributionInfoPayload,
    signature: ClientSignature,
}

impl WelcomeAttributionInfo {
    pub fn new(payload: WelcomeAttributionInfoPayload, signature: ClientSignature) -> Self {
        Self { payload, signature }
    }

    pub fn into_verifiable(
        self,
        group_id: GroupId,
        serialized_welcome: Vec<u8>,
    ) -> VerifiableWelcomeAttributionInfo {
        let tbs = WelcomeAttributionInfoTbs {
            payload: self.payload,
            group_id,
            welcome: serialized_welcome,
        };
        VerifiableWelcomeAttributionInfo {
            payload: tbs,
            signature: self.signature,
        }
    }
}

#[derive(Debug)]
pub struct VerifiableWelcomeAttributionInfo {
    payload: WelcomeAttributionInfoTbs,
    signature: ClientSignature,
}

impl VerifiableWelcomeAttributionInfo {
    pub fn sender(&self) -> UserId {
        self.payload.payload.sender_user_id.clone()
    }
}

impl Verifiable for VerifiableWelcomeAttributionInfo {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.payload.tls_serialize_detached()
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        &self.signature
    }

    fn label(&self) -> &str {
        "WelcomeAttributionInfo"
    }
}

mod private_mod {
    #[derive(Default)]
    pub struct Seal;
}

impl VerifiedStruct<VerifiableWelcomeAttributionInfo> for WelcomeAttributionInfoPayload {
    type SealingType = private_mod::Seal;

    fn from_verifiable(
        verifiable: VerifiableWelcomeAttributionInfo,
        _seal: Self::SealingType,
    ) -> Self {
        verifiable.payload.payload
    }
}

#[derive(Debug)]
pub struct EncryptedWelcomeAttributionInfoCtype;
pub type EncryptedWelcomeAttributionInfo = Ciphertext<EncryptedWelcomeAttributionInfoCtype>;

impl AeadEncryptable<WelcomeAttributionInfoEarKey, EncryptedWelcomeAttributionInfoCtype>
    for WelcomeAttributionInfo
{
}

impl AeadDecryptable<WelcomeAttributionInfoEarKey, EncryptedWelcomeAttributionInfoCtype>
    for WelcomeAttributionInfo
{
}
