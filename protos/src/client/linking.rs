// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Payloads of the linking messages that travel inside the pairing group.
//!
//! The message types are registered in
//! [`crate::relay_service::mdl::LinkingPayloadType`].

use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};

use crate::client::self_group::LinkedDevicePlatform;

/// The payload of the new device's `PairingConfirmed` message.
///
/// ## CDDL Definition
///
/// ```cddl
/// PairingConfirmation = {
///   ? 1: uint,          ; platform, omitted if unknown
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct PairingConfirmation {
    #[tag(1)]
    pub platform: LinkedDevicePlatform,
}

#[cfg(test)]
mod tests {
    use aircommon::codec::PersistenceCodec;

    use super::*;

    #[test]
    fn pairing_confirmation_roundtrip() {
        let confirmation = PairingConfirmation {
            platform: LinkedDevicePlatform::Macos,
        };
        let bytes = PersistenceCodec::to_vec(&confirmation).unwrap();
        let decoded: PairingConfirmation = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(decoded, confirmation);
    }

    #[test]
    fn pairing_confirmation_stability() {
        let confirmation = PairingConfirmation {
            platform: LinkedDevicePlatform::Ios,
        };
        let bytes = PersistenceCodec::to_vec(&confirmation).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }
}
