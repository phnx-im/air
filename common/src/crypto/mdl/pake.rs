// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The CPace exchange and key schedule of the multi-device linking
//! protocol.
//!
//! The new device is the initiator, the existing device the responder.
//! Both feed the linking password, the serialized channel identifier and
//! the session id into CPace, and post-process the resulting intermediate
//! session key into the pairing group's external PSK.

use cpace::{Context, InitiatorState, Msg};
use hkdf::Hkdf;
use secrecy::zeroize::Zeroize;
use sha2::{Digest, Sha256};

use super::code::LinkingPassword;

/// Salt of the HKDF extraction over the CPace intermediate session key.
const KEY_SCHEDULE_SALT: &[u8] = b"air-mdl-v1 key schedule";

/// Info prefix under which the external PSK secret is expanded.
const PSK_SECRET_LABEL: &[u8] = b"mls external psk";

/// Prefix of the PSK ID hash.
const PSK_ID_LABEL: &[u8] = b"air-mdl-v1 psk id";

/// Length of the pairing group's external PSK.
pub const PSK_SECRET_LEN: usize = 32;

/// Length of the pairing group's PSK ID.
pub const PSK_ID_LEN: usize = 32;

/// A CPace message that failed to parse, or a peer element that failed to
/// decode. Both are terminal for the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid cpace message from the peer")]
pub struct InvalidPakeMessage;

impl From<cpace::Error> for InvalidPakeMessage {
    fn from(_: cpace::Error) -> Self {
        Self
    }
}

impl From<cpace::MsgError> for InvalidPakeMessage {
    fn from(_: cpace::MsgError) -> Self {
        Self
    }
}

/// The pairing group's external PSK.
pub struct MdlPsk {
    secret: [u8; PSK_SECRET_LEN],
    id: [u8; PSK_ID_LEN],
}

impl MdlPsk {
    pub fn secret(&self) -> &[u8; PSK_SECRET_LEN] {
        &self.secret
    }

    pub fn id(&self) -> &[u8; PSK_ID_LEN] {
        &self.id
    }
}

impl Drop for MdlPsk {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

impl std::fmt::Debug for MdlPsk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MdlPsk")
            .field("id", &hex::encode(self.id))
            .finish_non_exhaustive()
    }
}

/// The intermediate session key of a finished CPace exchange.
pub struct MdlIsk(cpace::Isk);

impl MdlIsk {
    /// Derives the pairing group's PSK over the session transcript.
    pub fn derive_psk(&self, kdf_ctx: &[u8]) -> MdlPsk {
        let kdf = Hkdf::<Sha256>::new(Some(KEY_SCHEDULE_SALT), self.0.as_ref());
        let mut info = Vec::with_capacity(PSK_SECRET_LABEL.len() + kdf_ctx.len());
        info.extend_from_slice(PSK_SECRET_LABEL);
        info.extend_from_slice(kdf_ctx);

        let mut secret = [0u8; PSK_SECRET_LEN];
        // The output is one hash block, which HKDF always accepts.
        let expanded = kdf.expand(&info, &mut secret);
        debug_assert!(expanded.is_ok(), "psk secret expansion failed");

        MdlPsk {
            secret,
            id: psk_id(kdf_ctx),
        }
    }
}

impl std::fmt::Debug for MdlIsk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MdlIsk(<redacted>)")
    }
}

/// The pairing group's PSK ID, which depends on public values only.
fn psk_id(kdf_ctx: &[u8]) -> [u8; PSK_ID_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(PSK_ID_LABEL);
    hasher.update(kdf_ctx);
    hasher.finalize().into()
}

/// The new device's half of the exchange, between sending `MSGa` and
/// receiving `MSGb`.
pub struct MdlInitiator {
    state: InitiatorState,
    msg_a: Vec<u8>,
}

impl MdlInitiator {
    /// Starts the exchange.
    pub fn start(password: &LinkingPassword, ci: &[u8], sid: &[u8], ad_a: &[u8]) -> Self {
        let ctx = Context {
            prs: password.as_str().as_bytes(),
            ci,
            sid,
        };
        let (state, msg) = cpace::initiate(&ctx, ad_a, &mut rand::rng());
        Self {
            state,
            msg_a: msg.as_bytes().to_vec(),
        }
    }

    /// `MSGa`, which goes on the wire verbatim.
    pub fn msg_a(&self) -> &[u8] {
        &self.msg_a
    }

    /// Completes the exchange with the responder's `MSGb`.
    pub fn finish(self, msg_b: &[u8]) -> Result<MdlIsk, InvalidPakeMessage> {
        let msg_b = Msg::from_bytes(msg_b)?;
        let output = self.state.finish(&msg_b)?;
        Ok(MdlIsk(output.isk))
    }
}

impl std::fmt::Debug for MdlInitiator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MdlInitiator(<redacted>)")
    }
}

/// The existing device's output.
#[derive(Debug)]
pub struct MdlResponse {
    /// `MSGb`, which goes on the wire verbatim.
    pub msg_b: Vec<u8>,
    /// The initiator's associated data, its serialized KeyPackage.
    pub key_package: Vec<u8>,
    pub isk: MdlIsk,
}

/// Runs the existing device's half of the exchange over the received
/// `MSGa`.
pub fn respond(
    password: &LinkingPassword,
    ci: &[u8],
    sid: &[u8],
    msg_a: &[u8],
) -> Result<MdlResponse, InvalidPakeMessage> {
    let msg_a = Msg::from_bytes(msg_a)?;
    let ctx = Context {
        prs: password.as_str().as_bytes(),
        ci,
        sid,
    };
    let (msg_b, output) = cpace::respond(&ctx, b"", &msg_a, &mut rand::rng())?;
    Ok(MdlResponse {
        msg_b: msg_b.as_bytes().to_vec(),
        key_package: msg_a.associated_data().to_vec(),
        isk: MdlIsk(output.isk),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &[u8] = b"0123456789abcdef";
    const KEY_PACKAGE: &[u8] = b"a serialized key package";

    /// Runs a full exchange and returns both sides' PSKs. The KDF context
    /// stands in for the wire structure, which is defined a layer up.
    fn exchange(
        password_a: &LinkingPassword,
        password_b: &LinkingPassword,
        ci_a: &[u8],
        ci_b: &[u8],
    ) -> (MdlPsk, MdlPsk) {
        let initiator = MdlInitiator::start(password_a, ci_a, SID, KEY_PACKAGE);
        let msg_a = initiator.msg_a().to_vec();

        let response = respond(password_b, ci_b, SID, &msg_a).unwrap();
        assert_eq!(response.key_package, KEY_PACKAGE);

        let kdf_ctx_b = kdf_ctx(ci_b, &msg_a, &response.msg_b);
        let psk_b = response.isk.derive_psk(&kdf_ctx_b);

        let kdf_ctx_a = kdf_ctx(ci_a, &msg_a, &response.msg_b);
        let psk_a = initiator
            .finish(&response.msg_b)
            .unwrap()
            .derive_psk(&kdf_ctx_a);

        (psk_a, psk_b)
    }

    fn kdf_ctx(ci: &[u8], msg_a: &[u8], msg_b: &[u8]) -> Vec<u8> {
        [ci, SID, msg_a, msg_b].concat()
    }

    #[test]
    fn both_sides_agree() {
        let password = LinkingPassword::generate();
        let (a, b) = exchange(&password, &password, b"ci", b"ci");
        assert_eq!(a.secret(), b.secret());
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn a_wrong_password_keeps_the_psk_id_but_not_the_secret() {
        let (a, b) = exchange(
            &LinkingPassword::generate(),
            &LinkingPassword::generate(),
            b"ci",
            b"ci",
        );
        assert_eq!(a.id(), b.id());
        assert_ne!(a.secret(), b.secret());
    }

    #[test]
    fn a_different_channel_identifier_breaks_the_secret() {
        let password = LinkingPassword::generate();
        let (a, b) = exchange(&password, &password, b"ci-417", b"ci-418");
        assert_ne!(a.secret(), b.secret());
    }

    #[test]
    fn the_psk_id_covers_the_channel_identifier() {
        let password = LinkingPassword::generate();
        let initiator = MdlInitiator::start(&password, b"ci-417", SID, KEY_PACKAGE);
        let response = respond(&password, b"ci-417", SID, initiator.msg_a()).unwrap();
        let isk = initiator.finish(&response.msg_b).unwrap();

        let here = isk.derive_psk(&kdf_ctx(b"ci-417", &[], &response.msg_b));
        let elsewhere = isk.derive_psk(&kdf_ctx(b"ci-418", &[], &response.msg_b));
        assert_ne!(here.id(), elsewhere.id());
        assert_ne!(here.secret(), elsewhere.secret());
    }

    #[test]
    fn a_corrupted_share_is_rejected() {
        let password = LinkingPassword::generate();
        let initiator = MdlInitiator::start(&password, b"ci", SID, KEY_PACKAGE);
        let mut msg_a = initiator.msg_a().to_vec();
        msg_a.truncate(msg_a.len() - 1);
        assert_eq!(
            respond(&password, b"ci", SID, &msg_a).unwrap_err(),
            InvalidPakeMessage
        );

        let response = respond(&password, b"ci", SID, initiator.msg_a()).unwrap();
        let mut msg_b = response.msg_b.clone();
        msg_b[1] ^= 0xff;
        // A flipped element either fails to decode or yields a different
        // secret. Only the first is an error, so accept either outcome.
        if let Ok(isk) = initiator.finish(&msg_b) {
            let theirs = response.isk.derive_psk(b"ctx");
            assert_ne!(isk.derive_psk(b"ctx").secret(), theirs.secret());
        }
    }

    #[test]
    fn the_responder_reads_the_key_package_from_the_transcript() {
        let password = LinkingPassword::generate();
        let initiator = MdlInitiator::start(&password, b"ci", SID, KEY_PACKAGE);
        let response = respond(&password, b"ci", SID, initiator.msg_a()).unwrap();
        assert_eq!(response.key_package, KEY_PACKAGE);
    }
}
