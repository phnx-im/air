// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    crypto::ear::{EarDecryptable, EarEncryptable, keys::PushTokenEarKey},
    identifiers::TlsString,
};

use super::*;

#[derive(Serialize, Deserialize, TlsSize, TlsSerialize, TlsDeserializeBytes)]
#[repr(u8)]
pub enum PushTokenOperator {
    Apple,
    Google,
}

#[derive(Serialize, Deserialize, TlsSize, TlsSerialize, TlsDeserializeBytes)]
pub struct PushToken {
    operator: PushTokenOperator,
    token: TlsString,
}

impl PushToken {
    /// Create a new push token.
    pub fn new(operator: PushTokenOperator, token: String) -> Self {
        Self {
            operator,
            token: TlsString(token),
        }
    }

    pub fn operator(&self) -> &PushTokenOperator {
        &self.operator
    }

    pub fn token(&self) -> &str {
        &self.token.0
    }
}

#[derive(Debug)]
pub struct EncryptedPushTokenCtype;
pub type EncryptedPushToken = Ciphertext<EncryptedPushTokenCtype>;

impl EarEncryptable<PushTokenEarKey, EncryptedPushTokenCtype> for PushToken {}
impl EarDecryptable<PushTokenEarKey, EncryptedPushTokenCtype> for PushToken {}
