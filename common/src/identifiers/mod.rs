// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fmt, hash::Hash, str::FromStr};

use mls_assist::{openmls::group::GroupId, openmls_traits::types::HpkeCiphertext};
use rand::{CryptoRng, Rng};
use sqlx::{Database, Decode, Encode, Type, encode::IsNull, error::BoxDynError};
use tls_codec_impls::TlsUuid;
use tracing::{debug, error};
use url::Host;
use uuid::Uuid;

use crate::crypto::{
    ear::keys::PushTokenEarKey,
    hpke::{ClientIdKeyType, HpkeDecryptable, HpkeEncryptable},
};

pub use attachment::{AttachmentId, AttachmentIdParseError};
pub use mimi_id::{MimiId, MimiIdCalculationError};
pub use tls_codec_impls::{TlsStr, TlsString};
pub use user_handle::{
    USER_HANDLE_VALIDITY_PERIOD, UserHandle, UserHandleHash, UserHandleHashError,
    UserHandleValidationError,
};

use super::*;

mod attachment;
mod mimi_id;
mod tls_codec_impls;
mod user_handle;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fqdn {
    domain: Host<String>,
}

impl From<Host> for Fqdn {
    fn from(value: Host) -> Self {
        Self { domain: value }
    }
}

impl From<Fqdn> for String {
    fn from(value: Fqdn) -> Self {
        match value.domain {
            Host::Domain(domain) => domain,
            Host::Ipv4(addr) => addr.to_string(),
            Host::Ipv6(addr) => addr.to_string(),
        }
    }
}

impl<DB: Database> Type<DB> for Fqdn
where
    String: Type<DB>,
{
    fn type_info() -> DB::TypeInfo {
        <String as Type<DB>>::type_info()
    }
}

impl<'r, DB: Database> Encode<'r, DB> for Fqdn
where
    String: Encode<'r, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as Database>::ArgumentBuffer<'r>,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<DB>::encode(self.to_string(), buf)
    }
}

impl<'r, DB: Database> Decode<'r, DB> for Fqdn
where
    &'r str: Decode<'r, DB>,
{
    fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let s: &str = Decode::<DB>::decode(value)?;
        let fqdn = s.parse().map_err(|error| {
            error!(%error, "Error parsing Fqdn from DB");
            sqlx::Error::Decode(Box::new(error))
        })?;
        Ok(fqdn)
    }
}

impl fmt::Display for Fqdn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.domain)
    }
}

#[derive(Debug, Clone, Error)]
pub enum FqdnError {
    #[error("The given string does not represent a valid domain name.")]
    NotADomainName,
    #[error(transparent)]
    UrlError(#[from] url::ParseError),
}

impl FromStr for Fqdn {
    type Err = FqdnError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Arbitrary upper limit of 100 characters so we know it will cleanly tls-serialize.
        if s.len() > 100 {
            return Err(FqdnError::NotADomainName);
        }
        match Host::parse(s)? {
            domain @ Host::Domain(_) => Ok(Self { domain }),
            // Fqdns can't be IP addresses.
            Host::Ipv4(_) | Host::Ipv6(_) => Err(FqdnError::NotADomainName),
        }
    }
}

#[derive(Debug, Clone, PartialEq, TlsSerialize, TlsSize, TlsDeserializeBytes)]
pub struct QualifiedGroupId {
    group_uuid: [u8; 16],
    owning_domain: Fqdn,
}

impl QualifiedGroupId {
    pub fn new(uuid: Uuid, owning_domain: Fqdn) -> Self {
        let group_id = uuid.into_bytes();
        Self {
            group_uuid: group_id,
            owning_domain,
        }
    }

    pub fn group_uuid(&self) -> Uuid {
        Uuid::from_bytes(self.group_uuid)
    }

    pub fn owning_domain(&self) -> &Fqdn {
        &self.owning_domain
    }
}

impl TryFrom<GroupId> for QualifiedGroupId {
    type Error = tls_codec::Error;

    fn try_from(value: GroupId) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl TryFrom<&GroupId> for QualifiedGroupId {
    type Error = tls_codec::Error;

    fn try_from(value: &GroupId) -> Result<Self, Self::Error> {
        Self::tls_deserialize_exact_bytes(value.as_slice())
    }
}

impl From<QualifiedGroupId> for GroupId {
    fn from(value: QualifiedGroupId) -> Self {
        // We can unwrap here, because we know that neither the uuid nor the
        // domain will be too long to TLS-serialize.
        let group_id_bytes = value.tls_serialize_detached().unwrap();
        GroupId::from_slice(&group_id_bytes)
    }
}

impl fmt::Display for QualifiedGroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let uuid = Uuid::from_bytes(self.group_uuid);
        write!(f, "{}@{}", uuid, self.owning_domain)
    }
}

#[derive(Debug, Clone, Error)]
pub enum QualifiedGroupIdError {
    #[error(transparent)]
    FqdnError(#[from] FqdnError),
    #[error("The given string does not represent a valid qualified group id.")]
    InvalidQualifiedGroupId,
}

impl FromStr for QualifiedGroupId {
    type Err = QualifiedGroupIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split_string = s.split('@');
        let group_id = split_string.next().ok_or_else(|| {
            debug!("The given string is empty");
            QualifiedGroupIdError::InvalidQualifiedGroupId
        })?;

        let group_id_uuid: Uuid = group_id.parse().map_err(|_| {
            debug!("The given group id is not a valid UUID");
            QualifiedGroupIdError::InvalidQualifiedGroupId
        })?;
        let group_id = group_id_uuid.into_bytes();
        // GroupIds MUST be qualified
        let domain = split_string.next().ok_or_else(|| {
            debug!("The given group id is not qualified");
            QualifiedGroupIdError::InvalidQualifiedGroupId
        })?;
        let owning_domain = domain.parse()?;
        if split_string.next().is_some() {
            debug!("The domain name may not contain a '@'");
            return Err(QualifiedGroupIdError::InvalidQualifiedGroupId);
        }

        Ok(Self {
            group_uuid: group_id,
            owning_domain,
        })
    }
}

#[derive(
    Clone,
    Serialize,
    Deserialize,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    Hash,
    TlsSize,
    TlsSerialize,
    TlsDeserializeBytes,
)]
pub struct UserId {
    uuid: TlsUuid,
    domain: Fqdn,
}

impl fmt::Debug for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.uuid.0, self.domain)
    }
}

impl UserId {
    pub fn new(uuid: Uuid, domain: Fqdn) -> Self {
        Self {
            uuid: TlsUuid(uuid),
            domain,
        }
    }

    pub fn random(domain: Fqdn) -> Self {
        Self::new(Uuid::new_v4(), domain)
    }

    pub fn uuid(&self) -> Uuid {
        *self.uuid
    }

    pub fn domain(&self) -> &Fqdn {
        &self.domain
    }

    pub fn into_parts(self) -> (Uuid, Fqdn) {
        (*self.uuid, self.domain)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.tls_serialize_detached()
    }
}

#[derive(
    Clone,
    Debug,
    Serialize,
    Deserialize,
    TlsSerialize,
    TlsDeserializeBytes,
    TlsSize,
    PartialEq,
    Eq,
    Hash,
)]
pub struct QsReference {
    pub client_homeserver_domain: Fqdn,
    pub sealed_reference: SealedClientReference,
}

#[derive(
    Debug, Serialize, Deserialize, Clone, TlsSerialize, TlsDeserializeBytes, TlsSize, PartialEq, Eq,
)]
pub struct SealedClientReference {
    pub(crate) ciphertext: HpkeCiphertext,
}

impl SealedClientReference {
    pub fn into_ciphertext(self) -> HpkeCiphertext {
        self.ciphertext
    }
}

impl Hash for SealedClientReference {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ciphertext.kem_output.hash(state);
    }
}

impl From<HpkeCiphertext> for SealedClientReference {
    fn from(value: HpkeCiphertext) -> Self {
        Self { ciphertext: value }
    }
}

impl AsRef<HpkeCiphertext> for SealedClientReference {
    fn as_ref(&self) -> &HpkeCiphertext {
        &self.ciphertext
    }
}

/// Info describing the queue configuration for a member of a given group.
#[derive(TlsSerialize, TlsDeserializeBytes, TlsSize, Serialize, Deserialize, Clone)]
pub struct ClientConfig {
    pub client_id: QsClientId,
    // Some clients might not use push tokens.
    pub push_token_ear_key: Option<PushTokenEarKey>,
}

impl HpkeEncryptable<ClientIdKeyType, SealedClientReference> for ClientConfig {}
impl HpkeDecryptable<ClientIdKeyType, SealedClientReference> for ClientConfig {}

/// This is the pseudonymous client id used on the QS.
#[derive(
    Serialize,
    Deserialize,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    TlsSize,
    TlsSerialize,
    TlsDeserializeBytes,
    sqlx::Type, // Only for Postgres
)]
#[sqlx(transparent)]
pub struct QsClientId(TlsUuid);

impl QsClientId {
    pub fn random(rng: &mut (impl CryptoRng + Rng)) -> Self {
        let random_bytes = rng.r#gen::<[u8; 16]>();
        Uuid::from_bytes(random_bytes).into()
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl From<Uuid> for QsClientId {
    fn from(value: Uuid) -> Self {
        Self(TlsUuid(value))
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    Hash,
    TlsSize,
    TlsDeserializeBytes,
    TlsSerialize,
    sqlx::Type,
)]
#[sqlx(transparent)]
pub struct QsUserId(TlsUuid);

impl From<Uuid> for QsUserId {
    fn from(value: Uuid) -> Self {
        Self(TlsUuid(value))
    }
}

impl QsUserId {
    pub fn random() -> Self {
        Uuid::new_v4().into()
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_fqdn() {
        let fqdn_str = "example.com";
        let fqdn = Fqdn::from_str(fqdn_str).unwrap();
        assert_eq!(fqdn.domain, Host::Domain(fqdn_str.to_string()));

        let fqdn_subdomain_str = "sub.example.com";
        let fqdn = Fqdn::from_str(fqdn_subdomain_str).unwrap();
        assert_eq!(fqdn.domain, Host::Domain(fqdn_subdomain_str.to_string()));
    }

    #[test]
    fn invalid_fqdn() {
        let fqdn_str = "invalid#domain#character";
        let result = Fqdn::from_str(fqdn_str);
        assert!(result.is_err());
        assert!(matches!(result, Err(FqdnError::UrlError(_))));
    }

    #[test]
    fn ip_address_fqdn() {
        let fqdn_str = "192.168.0.1";
        let result = Fqdn::from_str(fqdn_str);
        assert!(matches!(result, Err(FqdnError::NotADomainName)));
    }
}
