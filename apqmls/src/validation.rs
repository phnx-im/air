// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Validation of the ciphersuites and the [`ApqInfo`] of an APQMLS session.
//!
//! The draft requires that the T session uses only classical primitives, that
//! the PQ session uses a standardized pure PQ KEM, and that the signature
//! algorithm of the PQ session matches the [`PqtMode`] of the session. It also
//! requires that both groups carry the same [`ApqInfo`].

use std::{collections::BTreeMap, fmt};

use openmls::{
    group::{GroupContext, GroupEpoch, GroupId, MlsGroup, ProcessedWelcome},
    prelude::{
        Ciphersuite, Credential, Extensions, HpkeKemType, SignatureScheme,
        group_info::VerifiableGroupInfo,
    },
    schedule::PreSharedKeyId,
};
use thiserror::Error;

use crate::{
    ApqCiphersuite,
    extension::{ApqInfo, PqtMode},
};

/// One of the two sessions of an APQMLS group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    T,
    Pq,
}

impl fmt::Display for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Session::T => f.write_str("T"),
            Session::Pq => f.write_str("PQ"),
        }
    }
}

/// Errors that can occur when validating an APQMLS session.
#[derive(Debug, Error)]
pub enum ApqValidationError {
    #[error("The T and the PQ session use the same ciphersuite {0:?}")]
    DuplicateCiphersuite(Ciphersuite),
    #[error("The T session must use a classical KEM, but uses {0:?}")]
    NonClassicalTKem(HpkeKemType),
    #[error("The T session must use a classical signature algorithm, but uses {0:?}")]
    NonClassicalTSignature(SignatureScheme),
    #[error("The PQ session must use a standardized pure PQ KEM, but uses {0:?}")]
    InvalidPqKem(HpkeKemType),
    #[error("The PQ session signature algorithm {signature_scheme:?} does not match mode {mode:?}")]
    ModeMismatch {
        mode: PqtMode,
        signature_scheme: SignatureScheme,
    },
    #[error("Missing APQInfo in the group context of the {0} session")]
    MissingApqInfo(Session),
    #[error("The APQInfo of the T and the PQ session do not match")]
    ApqInfoMismatch,
    #[error("The group ID of the {0} session does not match the one in APQInfo")]
    GroupIdMismatch(Session),
    #[error("The ciphersuite of the {0} session does not match the one in APQInfo")]
    CiphersuiteMismatch(Session),
    #[error("The epoch of the {0} session does not match the one in APQInfo")]
    EpochMismatch(Session),
    #[error("The APQInfo t_epoch {apq_info} is ahead of the T session epoch {session}")]
    TEpochAhead {
        apq_info: GroupEpoch,
        session: GroupEpoch,
    },
    #[error(
        "The two sessions have different members: the T session has {t_members} and the PQ \
         session has {pq_members}"
    )]
    MemberCountMismatch { t_members: usize, pq_members: usize },
    #[error("Leaf {0} is occupied in one session but not in the other")]
    LeafOccupancyMismatch(u32),
    #[error("The credentials at leaf {0} are not equivalent between the two sessions")]
    CredentialMismatch(u32),
    #[error("The T Welcome does not reference the PSK exported from the PQ session")]
    MissingApqPsk,
    #[error("Malformed APQInfo: {0}")]
    MalformedApqInfo(#[from] tls_codec::Error),
}

/// How a KEM is classified by the draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KemKind {
    Classical,
    /// A standardized KEM that is fully post-quantum.
    PurePq,
    /// A combiner of a classical and a post-quantum KEM.
    Hybrid,
}

fn kem_kind(kem: HpkeKemType) -> KemKind {
    match kem {
        HpkeKemType::DhKemP256
        | HpkeKemType::DhKemP384
        | HpkeKemType::DhKemP521
        | HpkeKemType::DhKem25519
        | HpkeKemType::DhKem448 => KemKind::Classical,
        HpkeKemType::MlKem768 | HpkeKemType::MlKem1024 => KemKind::PurePq,
        HpkeKemType::XWingKemDraft6 => KemKind::Hybrid,
    }
}

fn signature_is_pq(signature_scheme: SignatureScheme) -> bool {
    match signature_scheme {
        SignatureScheme::ECDSA_SECP256R1_SHA256
        | SignatureScheme::ECDSA_SECP384R1_SHA384
        | SignatureScheme::ECDSA_SECP521R1_SHA512
        | SignatureScheme::ED25519
        | SignatureScheme::ED448 => false,
        SignatureScheme::MLDSA44 | SignatureScheme::MLDSA65 | SignatureScheme::MLDSA87 => true,
    }
}

/// Returns the [`PqtMode`] implied by the signature algorithm of the PQ
/// session's ciphersuite.
pub fn implied_mode(pq_ciphersuite: Ciphersuite) -> PqtMode {
    if signature_is_pq(pq_ciphersuite.signature_algorithm()) {
        PqtMode::ConfAndAuth
    } else {
        PqtMode::ConfOnly
    }
}

/// Checks the ciphersuites of an APQMLS session against the rules of the draft.
///
/// The mode must match the signature algorithm of the PQ session: ML-DSA for
/// [`PqtMode::ConfAndAuth`] and a classical algorithm for
/// [`PqtMode::ConfOnly`].
pub fn validate_ciphersuites(
    ciphersuite: ApqCiphersuite,
    mode: PqtMode,
) -> Result<(), ApqValidationError> {
    let t = ciphersuite.t_ciphersuite();
    let pq = ciphersuite.pq_ciphersuite();

    if t == pq {
        return Err(ApqValidationError::DuplicateCiphersuite(t));
    }

    let t_kem = t.hpke_kem_algorithm();
    if kem_kind(t_kem) != KemKind::Classical {
        return Err(ApqValidationError::NonClassicalTKem(t_kem));
    }
    let t_signature = t.signature_algorithm();
    if signature_is_pq(t_signature) {
        return Err(ApqValidationError::NonClassicalTSignature(t_signature));
    }

    // Hybrid KEMs only add overhead here, since the PQ cost is already
    // amortized by the combiner itself.
    let pq_kem = pq.hpke_kem_algorithm();
    if kem_kind(pq_kem) != KemKind::PurePq {
        return Err(ApqValidationError::InvalidPqKem(pq_kem));
    }

    if implied_mode(pq) != mode {
        return Err(ApqValidationError::ModeMismatch {
            mode,
            signature_scheme: pq.signature_algorithm(),
        });
    }

    Ok(())
}

/// Validates the [`ApqInfo`] of a joined APQMLS session and returns it.
///
/// Both group contexts must carry the same [`ApqInfo`], it must describe the
/// two groups at hand, and its ciphersuites must pass
/// [`validate_ciphersuites`]. Callers that drive a join themselves, instead of
/// going through [`crate::ApqMlsGroup::new_from_welcome`], must call this once
/// both groups exist and before the join is committed.
///
/// `APQInfo.pq_epoch` must name the current PQ epoch, but `APQInfo.t_epoch` is
/// only required not to run ahead of the T group: it names the T epoch of the
/// last FULL commit and goes stale while PARTIAL commits advance the T group.
/// Use [`validate_apq_session_at_construction`] on paths that build a session
/// from scratch, where the T epoch does have to line up as well.
pub fn validate_apq_session(
    t_group: &MlsGroup,
    pq_group: &MlsGroup,
) -> Result<ApqInfo, ApqValidationError> {
    validate_apq_info(
        &ApqSessionRef::from_t_group(t_group),
        &ApqSessionRef::from_pq_group(pq_group),
    )
}

/// Same as [`validate_apq_session`], plus the checks that only hold on a path
/// that constructs the paired session, i.e. a Welcome or an external join.
///
/// On those paths the session starts at the epochs named in the [`ApqInfo`], no
/// PARTIAL commit has happened yet, and the two groups must already have the
/// same members.
pub fn validate_apq_session_at_construction(
    t_group: &MlsGroup,
    pq_group: &MlsGroup,
    credential_equivalence: impl Fn(&Credential, &Credential) -> bool,
) -> Result<ApqInfo, ApqValidationError> {
    let t = ApqSessionRef::from_t_group(t_group);
    let pq = ApqSessionRef::from_pq_group(pq_group);
    let apq_info = validate_apq_info(&t, &pq)?;
    if apq_info.t_epoch != t.epoch {
        return Err(ApqValidationError::EpochMismatch(t.session));
    }
    validate_membership(t_group, pq_group, credential_equivalence)?;
    Ok(apq_info)
}

/// The two sessions must contain the same members.
///
/// This compares the shape of the two trees, i.e. the member count and which
/// leaves are occupied, and hands each pair of members to
/// `credential_equivalence`.
pub fn validate_membership(
    t_group: &MlsGroup,
    pq_group: &MlsGroup,
    credential_equivalence: impl Fn(&Credential, &Credential) -> bool,
) -> Result<(), ApqValidationError> {
    let members = |group: &MlsGroup| -> BTreeMap<u32, Credential> {
        group
            .members()
            .map(|member| (member.index.u32(), member.credential))
            .collect()
    };
    let t_members = members(t_group);
    let mut pq_members = members(pq_group);

    if t_members.len() != pq_members.len() {
        return Err(ApqValidationError::MemberCountMismatch {
            t_members: t_members.len(),
            pq_members: pq_members.len(),
        });
    }

    for (index, t_credential) in &t_members {
        // A leaf that is blank in one session but not the other means the two
        // have diverged, even if the member counts happen to agree.
        let pq_credential = pq_members
            .remove(index)
            .ok_or(ApqValidationError::LeafOccupancyMismatch(*index))?;
        if !credential_equivalence(t_credential, &pq_credential) {
            return Err(ApqValidationError::CredentialMismatch(*index));
        }
    }

    Ok(())
}

/// Checks that the T Welcome imports the PSK exported from the PQ session.
///
/// The T Welcome of a paired join carries `apq_psk_id` in its `GroupSecrets`,
/// so a joiner whose T session would end up without any PQ contribution can
/// tell before it joins. `apq_psk_id` is the value the joiner derived from the
/// PQ session it just joined, i.e. the return value of
/// [`crate::welcome::derive_and_store_join_psk`]. Only the PSK itself is
/// compared, not the nonce, which the sender picks.
pub fn validate_welcome_psk(
    processed_t_welcome: &ProcessedWelcome,
    apq_psk_id: &PreSharedKeyId,
) -> Result<(), ApqValidationError> {
    processed_t_welcome
        .psks()
        .iter()
        .any(|psk_id| psk_id.psk() == apq_psk_id.psk())
        .then_some(())
        .ok_or(ApqValidationError::MissingApqPsk)
}

/// Validates the [`ApqInfo`] of the two `GroupInfo`s an external joiner is
/// about to build its external commits from, and returns it.
///
/// This runs the same checks as [`validate_apq_session`], so an external joiner
/// enforces the APQ invariants before it commits to anything.
pub fn validate_apq_group_info(
    t_group_info: &VerifiableGroupInfo,
    pq_group_info: &VerifiableGroupInfo,
) -> Result<ApqInfo, ApqValidationError> {
    validate_apq_info(
        &ApqSessionRef::from_group_context(Session::T, t_group_info.group_context()),
        &ApqSessionRef::from_group_context(Session::Pq, pq_group_info.group_context()),
    )
}

/// A borrowed view of one leg of an APQ session.
struct ApqSessionRef<'a> {
    session: Session,
    group_id: &'a GroupId,
    epoch: GroupEpoch,
    ciphersuite: Ciphersuite,
    extensions: &'a Extensions<GroupContext>,
}

impl<'a> ApqSessionRef<'a> {
    fn from_t_group(group: &'a MlsGroup) -> Self {
        Self {
            session: Session::T,
            group_id: group.group_id(),
            epoch: group.epoch(),
            ciphersuite: group.ciphersuite(),
            extensions: group.extensions(),
        }
    }

    fn from_pq_group(group: &'a MlsGroup) -> Self {
        Self {
            session: Session::Pq,
            group_id: group.group_id(),
            epoch: group.epoch(),
            ciphersuite: group.ciphersuite(),
            extensions: group.extensions(),
        }
    }

    fn from_group_context(session: Session, group_context: &'a GroupContext) -> Self {
        Self {
            session,
            group_id: group_context.group_id(),
            epoch: group_context.epoch(),
            ciphersuite: group_context.ciphersuite(),
            extensions: group_context.extensions(),
        }
    }
}

/// The [`ApqInfo`] checks that hold in every epoch of a session.
fn validate_apq_info(
    t: &ApqSessionRef<'_>,
    pq: &ApqSessionRef<'_>,
) -> Result<ApqInfo, ApqValidationError> {
    let t_info = ApqInfo::from_extensions(t.extensions)?
        .ok_or(ApqValidationError::MissingApqInfo(t.session))?;
    let pq_info = ApqInfo::from_extensions(pq.extensions)?
        .ok_or(ApqValidationError::MissingApqInfo(pq.session))?;

    if t_info != pq_info {
        return Err(ApqValidationError::ApqInfoMismatch);
    }

    if &t_info.t_session_group_id != t.group_id {
        return Err(ApqValidationError::GroupIdMismatch(t.session));
    }
    if &t_info.pq_session_group_id != pq.group_id {
        return Err(ApqValidationError::GroupIdMismatch(pq.session));
    }
    if t_info.t_cipher_suite != t.ciphersuite {
        return Err(ApqValidationError::CiphersuiteMismatch(t.session));
    }
    if t_info.pq_cipher_suite != pq.ciphersuite {
        return Err(ApqValidationError::CiphersuiteMismatch(pq.session));
    }

    validate_ciphersuites(
        ApqCiphersuite::new(t.ciphersuite, pq.ciphersuite),
        t_info.mode,
    )?;

    // The PQ session only ever advances as part of a FULL commit, and every
    // FULL commit sets APQInfo to the new epochs, so this holds in every epoch.
    if t_info.pq_epoch != pq.epoch {
        return Err(ApqValidationError::EpochMismatch(pq.session));
    }
    // The T session also advances on PARTIAL commits, which leave APQInfo
    // alone, so `t_epoch` may lag. It must never lead.
    if t_info.t_epoch > t.epoch {
        return Err(ApqValidationError::TEpochAhead {
            apq_info: t_info.t_epoch,
            session: t.epoch,
        });
    }

    Ok(t_info)
}
