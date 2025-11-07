// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::tls_codec::{Deserialize, DeserializeBytes, Error as TlsCodecError, Serialize, Size};
use openmls::{
    prelude::{MlsMessageBodyIn, MlsMessageIn, MlsMessageOut, ProtocolMessage, WireFormat},
    versions::ProtocolVersion,
};

use super::{AssistedGroupInfoIn, AssistedMessageIn, AssistedWelcome};

impl Size for AssistedMessageIn {
    fn tls_serialized_len(&self) -> usize {
        ProtocolVersion::default().tls_serialized_len()
            + match &self.mls_message {
                ProtocolMessage::PrivateMessage(pm) => {
                    WireFormat::PrivateMessage.tls_serialized_len() + pm.tls_serialized_len()
                }
                ProtocolMessage::PublicMessage(pm) => {
                    WireFormat::PublicMessage.tls_serialized_len() + pm.tls_serialized_len()
                }
            }
            + self.group_info_option.tls_serialized_len()
    }
}

impl DeserializeBytes for AssistedMessageIn {
    fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), TlsCodecError>
    where
        Self: Sized,
    {
        let (mls_message, remainder) =
            <MlsMessageIn as DeserializeBytes>::tls_deserialize_bytes(bytes)?;
        let mut remainder_reader = remainder;
        let group_info_option =
            Option::<AssistedGroupInfoIn>::tls_deserialize(&mut remainder_reader)?;
        let serialized_mls_message = bytes
            .get(..bytes.len() - remainder.len())
            .ok_or(TlsCodecError::EndOfStream)?
            .to_vec();
        let remainder = remainder
            .get(group_info_option.tls_serialized_len()..)
            .ok_or(TlsCodecError::EndOfStream)?;
        let mls_message = match mls_message.extract() {
            MlsMessageBodyIn::PublicMessage(pm) => pm.into(),
            MlsMessageBodyIn::PrivateMessage(pm) => pm.into(),
            MlsMessageBodyIn::Welcome(_)
            | MlsMessageBodyIn::GroupInfo(_)
            | MlsMessageBodyIn::KeyPackage(_) => return Err(TlsCodecError::InvalidInput),
        };

        let assisted_message = Self {
            mls_message,
            serialized_mls_message: super::SerializedMlsMessage(serialized_mls_message),
            group_info_option,
        };
        Ok((assisted_message, remainder))
    }
}

impl Size for AssistedWelcome {
    fn tls_serialized_len(&self) -> usize {
        MlsMessageOut::from_welcome(self.welcome.clone(), ProtocolVersion::default())
            .tls_serialized_len()
        //// Any version
        //ProtocolVersion::default().tls_serialized_len() +
        //// Any wire format
        //WireFormat::PublicMessage.tls_serialized_len() +
        //// The welcome
        //self.welcome.tls_serialized_len()
    }
}

impl Serialize for AssistedWelcome {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, TlsCodecError> {
        MlsMessageOut::from_welcome(self.welcome.clone(), ProtocolVersion::default())
            .tls_serialize(writer)
    }
}

impl Deserialize for AssistedWelcome {
    fn tls_deserialize<R: std::io::Read>(bytes: &mut R) -> Result<Self, TlsCodecError>
    where
        Self: Sized,
    {
        let mls_message = <MlsMessageIn as Deserialize>::tls_deserialize(bytes)?;
        match mls_message.extract() {
            MlsMessageBodyIn::Welcome(welcome) => Ok(AssistedWelcome { welcome }),
            _ => Err(TlsCodecError::InvalidInput),
        }
    }
}

impl DeserializeBytes for AssistedWelcome {
    fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), TlsCodecError>
    where
        Self: Sized,
    {
        let (mls_message, remainder) =
            <MlsMessageIn as DeserializeBytes>::tls_deserialize_bytes(bytes)?;
        match mls_message.extract() {
            MlsMessageBodyIn::Welcome(welcome) => Ok((AssistedWelcome { welcome }, remainder)),
            _ => Err(TlsCodecError::EndOfStream),
        }
    }
}
