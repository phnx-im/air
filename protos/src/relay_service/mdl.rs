// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Wire types of the multi-device linking protocol.
//!
//! Every `RelayFrame` payload of a linking session carries exactly one
//! TLS-serialized [`MdlMessage`]. The relay parses only
//! [`ProvisionRequest`] and produces [`SessionAssigned`]. Every other
//! message is opaque to it.

use tls_codec::{
    DeserializeBytes, Serialize, TlsDeserializeBytes, TlsSerialize, TlsSize, VLByteSlice,
};

use super::v1::RelayFrame;

/// The protocol version this implementation speaks.
pub const MDL_PROTOCOL_VERSION: u16 = 1;

/// CPace ciphersuite bound into [`MdlContext`]: CPACE-RISTR255-SHA512.
pub const MDL_PAKE_CIPHER_SUITE: u16 = 1;

/// MLS ciphersuite of the pairing group bound into [`MdlContext`]:
/// `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`.
pub const MDL_MLS_CIPHER_SUITE: u16 = 0x0001;

/// Domain separator of the protocol, bound into [`MdlContext`].
pub const MDL_PROTOCOL_LABEL: &str = "air multi-device linking";

/// The new device's role label, also its basic-credential identity in the
/// pairing group.
pub const MDL_INITIATOR_LABEL: &str = "new-device";

/// The existing device's role label, also its basic-credential identity in
/// the pairing group.
pub const MDL_RESPONDER_LABEL: &str = "existing-device";

/// One message of a linking session.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
#[repr(u8)]
pub enum MdlMessage {
    #[tls_codec(discriminant = 1)]
    ProvisionRequest(ProvisionRequest),
    #[tls_codec(discriminant = 2)]
    SessionAssigned(SessionAssigned),
    #[tls_codec(discriminant = 3)]
    PakeShareA(PakeShareA),
    #[tls_codec(discriminant = 4)]
    PakeShareB(PakeShareB),
    #[tls_codec(discriminant = 5)]
    GroupMessage(GroupMessage),
    #[tls_codec(discriminant = 6)]
    Abort(Abort),
}

impl MdlMessage {
    /// Wraps this message into a relay frame.
    pub fn into_frame(self) -> Result<RelayFrame, tls_codec::Error> {
        Ok(self.tls_serialize_detached()?.into())
    }

    /// Parses a relay frame's payload.
    pub fn from_frame(frame: &RelayFrame) -> Result<Self, tls_codec::Error> {
        Self::tls_deserialize_exact_bytes(frame.as_slice())
    }
}

/// Opens a rendezvous session. Carries no PAKE share: the CPace generator
/// depends on the rendezvous ID, which the new device does not know yet.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct ProvisionRequest {
    pub version: u16,
}

/// The relay's answer to a [`ProvisionRequest`].
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct SessionAssigned {
    pub rendezvous_id: String,
}

/// The initiator's CPace message, which carries the new device's KeyPackage
/// as its associated data.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct PakeShareA {
    /// The version the new device speaks. The relay checked it already, but
    /// the existing device never sees that exchange, and without the field a
    /// mismatch would surface as a wrong password.
    pub version: u16,
    pub sid: Vec<u8>,
    pub msg_a: Vec<u8>,
}

/// The responder's CPace message together with the pairing group's Welcome.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct PakeShareB {
    pub msg_b: Vec<u8>,
    pub welcome: Vec<u8>,
}

/// An MLS message of the pairing group.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct GroupMessage {
    pub mls_message: Vec<u8>,
}

/// Ends the session. Terminal for both peers.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct Abort {
    pub code: AbortCode,
}

/// Why a linking session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
#[repr(u8)]
pub enum AbortCode {
    /// Welcome decryption or key-schedule failure, meaning a wrong password.
    AuthenticationFailed = 1,
    /// Unexpected or malformed message, or an MLS processing error.
    ProtocolError = 2,
    /// A KeyPackage, Welcome or group check failed.
    ValidationFailed = 3,
    /// The user declined the confirmation dialog.
    UserRejected = 4,
    Timeout = 5,
    /// The peer speaks a protocol version this device does not.
    UnsupportedVersion = 6,
}

/// The CPace channel identifier, binding the exchange to the protocol
/// version, both ciphersuites, both roles, the server and the session.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsSize)]
pub struct MdlContext {
    pub protocol_label: String,
    pub protocol_version: u16,
    pub pake_cipher_suite: u16,
    pub mls_cipher_suite: u16,
    pub initiator_label: String,
    pub responder_label: String,
    pub server_domain: String,
    pub rendezvous_id: String,
}

impl MdlContext {
    /// The context for a session on `server_domain` under `rendezvous_id`,
    /// with every other field fixed by this implementation.
    ///
    /// `server_domain` must be the lowercase FQDN of the relay's server.
    pub fn new(server_domain: String, rendezvous_id: String) -> Self {
        Self {
            protocol_label: MDL_PROTOCOL_LABEL.to_owned(),
            protocol_version: MDL_PROTOCOL_VERSION,
            pake_cipher_suite: MDL_PAKE_CIPHER_SUITE,
            mls_cipher_suite: MDL_MLS_CIPHER_SUITE,
            initiator_label: MDL_INITIATOR_LABEL.to_owned(),
            responder_label: MDL_RESPONDER_LABEL.to_owned(),
            server_domain,
            rendezvous_id,
        }
    }
}

/// The transcript the pairing group's PSK is derived over.
///
/// The CPace messages enter as opaque byte strings, so the transcript is
/// bound without re-deriving CPace's length-value encoding.
#[derive(Debug, TlsSerialize, TlsSize)]
pub struct MdlKdfContext<'a> {
    /// The TLS-serialized [`MdlContext`].
    pub ci: VLByteSlice<'a>,
    pub sid: VLByteSlice<'a>,
    /// `MSGa`, as sent respectively received.
    pub msg_a: VLByteSlice<'a>,
    /// `MSGb`, as sent respectively received.
    pub msg_b: VLByteSlice<'a>,
}

/// The plaintext of a linking-payload application message.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct LinkingPayload {
    pub payload_type: LinkingPayloadType,
    pub payload: Vec<u8>,
}

/// The provisional registry of linking-payload types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
#[repr(u16)]
pub enum LinkingPayloadType {
    /// The existing device's account material, a CBOR `ProvisioningPackage`.
    ProvisioningPackage = 1,
    /// The new device's self-group KeyPackage, a CBOR `SelfGroupJoinRequest`.
    SelfGroupJoinRequest = 2,
    /// The new device joined the self group. Empty payload, both sides tear
    /// the session down afterwards.
    LinkingComplete = 3,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(message: MdlMessage) {
        let frame = message.clone().into_frame().unwrap();
        assert_eq!(MdlMessage::from_frame(&frame).unwrap(), message);
    }

    #[test]
    fn every_message_round_trips() {
        round_trip(MdlMessage::ProvisionRequest(ProvisionRequest {
            version: MDL_PROTOCOL_VERSION,
        }));
        round_trip(MdlMessage::SessionAssigned(SessionAssigned {
            rendezvous_id: "417".to_owned(),
        }));
        round_trip(MdlMessage::PakeShareA(PakeShareA {
            version: MDL_PROTOCOL_VERSION,
            sid: vec![7; 16],
            msg_a: vec![0xab; 70],
        }));
        round_trip(MdlMessage::PakeShareB(PakeShareB {
            msg_b: vec![0xcd; 34],
            welcome: vec![1, 2, 3],
        }));
        round_trip(MdlMessage::GroupMessage(GroupMessage {
            mls_message: vec![4, 5, 6],
        }));
        round_trip(MdlMessage::Abort(Abort {
            code: AbortCode::AuthenticationFailed,
        }));
    }

    #[test]
    fn message_types_use_the_fixed_discriminants() {
        let types: Vec<u8> = [
            MdlMessage::ProvisionRequest(ProvisionRequest { version: 1 }),
            MdlMessage::SessionAssigned(SessionAssigned {
                rendezvous_id: String::new(),
            }),
            MdlMessage::PakeShareA(PakeShareA {
                version: 1,
                sid: Vec::new(),
                msg_a: Vec::new(),
            }),
            MdlMessage::PakeShareB(PakeShareB {
                msg_b: Vec::new(),
                welcome: Vec::new(),
            }),
            MdlMessage::GroupMessage(GroupMessage {
                mls_message: Vec::new(),
            }),
            MdlMessage::Abort(Abort {
                code: AbortCode::Timeout,
            }),
        ]
        .into_iter()
        .map(|message| message.tls_serialize_detached().unwrap()[0])
        .collect();
        assert_eq!(types, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn unknown_message_type_is_rejected() {
        assert!(MdlMessage::from_frame(&RelayFrame::from(vec![7, 0, 1])).is_err());
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = MdlMessage::Abort(Abort {
            code: AbortCode::Timeout,
        })
        .tls_serialize_detached()
        .unwrap();
        bytes.push(0);
        assert!(MdlMessage::from_frame(&RelayFrame::from(bytes)).is_err());
    }

    #[test]
    fn unknown_abort_code_is_rejected() {
        assert!(MdlMessage::from_frame(&RelayFrame::from(vec![6, 9])).is_err());
    }

    #[test]
    fn unknown_linking_payload_type_is_rejected() {
        assert!(LinkingPayload::tls_deserialize_exact_bytes(&[0, 9, 0]).is_err());
    }

    #[test]
    fn linking_payload_round_trips() {
        let payload = LinkingPayload {
            payload_type: LinkingPayloadType::ProvisioningPackage,
            payload: vec![9; 5],
        };
        let bytes = payload.tls_serialize_detached().unwrap();
        assert_eq!(
            LinkingPayload::tls_deserialize_exact_bytes(&bytes).unwrap(),
            payload
        );
    }

    /// The context is what both sides must derive identically, so its
    /// serialization is fixed by the protocol rather than by this struct.
    #[test]
    fn context_serialization_is_stable() {
        let ci = MdlContext::new("example.com".to_owned(), "417".to_owned())
            .tls_serialize_detached()
            .unwrap();
        let mut expected = Vec::new();
        expected.push(MDL_PROTOCOL_LABEL.len() as u8);
        expected.extend_from_slice(MDL_PROTOCOL_LABEL.as_bytes());
        expected.extend_from_slice(&MDL_PROTOCOL_VERSION.to_be_bytes());
        expected.extend_from_slice(&MDL_PAKE_CIPHER_SUITE.to_be_bytes());
        expected.extend_from_slice(&MDL_MLS_CIPHER_SUITE.to_be_bytes());
        expected.push(MDL_INITIATOR_LABEL.len() as u8);
        expected.extend_from_slice(MDL_INITIATOR_LABEL.as_bytes());
        expected.push(MDL_RESPONDER_LABEL.len() as u8);
        expected.extend_from_slice(MDL_RESPONDER_LABEL.as_bytes());
        expected.push(11);
        expected.extend_from_slice(b"example.com");
        expected.push(3);
        expected.extend_from_slice(b"417");
        assert_eq!(ci, expected);
    }

    #[test]
    fn kdf_context_serialization_is_stable() {
        let kdf_ctx = MdlKdfContext {
            ci: VLByteSlice(&[1, 2]),
            sid: VLByteSlice(&[3]),
            msg_a: VLByteSlice(&[4, 5, 6]),
            msg_b: VLByteSlice(&[]),
        }
        .tls_serialize_detached()
        .unwrap();
        assert_eq!(kdf_ctx, vec![2, 1, 2, 1, 3, 3, 4, 5, 6, 0]);
    }
}
