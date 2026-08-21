// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::{
    component::{ComponentData, ComponentId, ComponentType},
    group::{GroupContext, GroupEpoch, GroupId},
    prelude::{
        AppDataDictionary, AppDataDictionaryExtension, AppDataUpdateProposal, Capabilities,
        Ciphersuite, Extension, ExtensionType, Extensions, LeafNode, LeafNodeParameters,
        ProposalType,
    },
};
use tap::Pipe;
use thiserror::Error;
use tls_codec::{Deserialize as _, Serialize as _, TlsDeserialize, TlsSerialize, TlsSize};

use crate::{
    ApqCiphersuite, ApqGroupId, ApqMlsGroup, ApqMlsGroupMut,
    key_package::ensure_ciphersuite_support,
};

/// The component ID of the APQMLS component.
///
/// The value is not yet finalized in the draft
/// <https://datatracker.ietf.org/doc/html/draft-ietf-mls-combiner#name-key-schedule>.
pub const APQMLS_COMPONENT_ID: ComponentId = 0x8001;

/// The mode of an [`ApqMlsGroup`], which determines whether only confidentiality or both
/// confidentiality and authentication is PQ secure.
#[derive(Default, Debug, Clone, TlsSize, TlsSerialize, TlsDeserialize, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PqtMode {
    #[default]
    ConfOnly,
    ConfAndAuth,
}

impl From<PqtMode> for bool {
    fn from(value: PqtMode) -> Self {
        match value {
            PqtMode::ConfOnly => false,
            PqtMode::ConfAndAuth => true,
        }
    }
}

impl PqtMode {
    /// Returns the default ciphersuite for the given mode.
    pub fn default_ciphersuite(&self) -> ApqCiphersuite {
        match self {
            PqtMode::ConfOnly => ApqCiphersuite::default_pq_conf(),
            PqtMode::ConfAndAuth => ApqCiphersuite::default_pq_conf_and_auth(),
        }
    }
}

/// The APQMLS extension, which is used to store APQMLS-specific information
/// in the extensions of an [`openmls::group::MlsGroup`].
#[derive(Debug, Clone, TlsSize, TlsSerialize, TlsDeserialize, PartialEq, Eq)]
pub struct ApqInfo {
    pub t_session_group_id: GroupId,
    pub pq_session_group_id: GroupId,
    pub mode: PqtMode,
    pub t_cipher_suite: Ciphersuite,
    pub pq_cipher_suite: Ciphersuite,
    pub t_epoch: GroupEpoch,
    pub pq_epoch: GroupEpoch,
}

impl ApqInfo {
    pub(super) fn to_component_data(&self) -> Result<ComponentData, tls_codec::Error> {
        let bytes = self.tls_serialize_detached()?;
        Ok(ComponentData::from_parts(APQMLS_COMPONENT_ID, bytes.into()))
    }

    /// The `AppDataUpdate` proposal that carries this [`ApqInfo`] as a
    /// `full_update`.
    pub(super) fn to_full_update_proposal(
        &self,
    ) -> Result<AppDataUpdateProposal, tls_codec::Error> {
        let update = ApqInfoUpdate::FullUpdate(self.clone()).tls_serialize_detached()?;
        Ok(AppDataUpdateProposal::update(APQMLS_COMPONENT_ID, update))
    }

    pub(super) fn set_epoch(&mut self, t_epoch: GroupEpoch, pq_epoch: GroupEpoch) {
        self.t_epoch = t_epoch;
        self.pq_epoch = pq_epoch;
    }

    /// Whether all fields that are immutable for the lifetime of the session
    /// match those of `other`.
    pub(super) fn matches_except_epochs(&self, other: &Self) -> bool {
        // Destructured so that a new field forces a decision here.
        let Self {
            t_session_group_id,
            pq_session_group_id,
            mode,
            t_cipher_suite,
            pq_cipher_suite,
            t_epoch: _,
            pq_epoch: _,
        } = self;
        *t_session_group_id == other.t_session_group_id
            && *pq_session_group_id == other.pq_session_group_id
            && *mode == other.mode
            && *t_cipher_suite == other.t_cipher_suite
            && *pq_cipher_suite == other.pq_cipher_suite
    }

    pub fn from_extensions(
        extensions: &Extensions<GroupContext>,
    ) -> Result<Option<Self>, tls_codec::Error> {
        extensions
            .app_data_dictionary()
            .and_then(|dict| dict.dictionary().get(&APQMLS_COMPONENT_ID))
            .map(ApqInfo::tls_deserialize_exact)
            .transpose()
    }

    pub fn group_id(&self) -> ApqGroupId {
        ApqGroupId {
            t_group_id: self.t_session_group_id.clone(),
            pq_group_id: self.pq_session_group_id.clone(),
        }
    }
}

/// The payload of an `AppDataUpdate` proposal for [`APQMLS_COMPONENT_ID`].
///
/// The entry in the group context app data dictionary is a bare [`ApqInfo`].
/// Only the proposal payload is an [`ApqInfoUpdate`].
#[derive(Debug, Clone, TlsSize, TlsSerialize, TlsDeserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum ApqInfoUpdate {
    #[tls_codec(discriminant = 0)]
    FullUpdate(ApqInfo) = 0,
    #[tls_codec(discriminant = 1)]
    NewTEpoch(GroupEpoch) = 1,
    #[tls_codec(discriminant = 2)]
    NewPqEpoch(GroupEpoch) = 2,
}

impl ApqInfoUpdate {
    /// Applies the update to the [`ApqInfo`] of the current epoch, yielding the
    /// [`ApqInfo`] of the new epoch.
    ///
    /// The epoch-only variants require an existing [`ApqInfo`].
    pub fn apply(self, current: Option<&ApqInfo>) -> Result<ApqInfo, ApqInfoUpdateError> {
        match self {
            ApqInfoUpdate::FullUpdate(new_apq_info) => Ok(new_apq_info),
            ApqInfoUpdate::NewTEpoch(t_epoch) => {
                let mut apq_info = current.cloned().ok_or(ApqInfoUpdateError::NoApqInfo)?;
                apq_info.t_epoch = t_epoch;
                Ok(apq_info)
            }
            ApqInfoUpdate::NewPqEpoch(pq_epoch) => {
                let mut apq_info = current.cloned().ok_or(ApqInfoUpdateError::NoApqInfo)?;
                apq_info.pq_epoch = pq_epoch;
                Ok(apq_info)
            }
        }
    }
}

/// Errors that can occur when applying an [`ApqInfoUpdate`].
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum ApqInfoUpdateError {
    #[error("The AppDataUpdate payload is not a valid APQInfoUpdate: {0}")]
    MalformedUpdate(tls_codec::Error),
    #[error("The APQInfo in the group context is malformed: {0}")]
    MalformedApqInfo(tls_codec::Error),
    #[error("An epoch-only APQInfo update requires an existing APQInfo.")]
    NoApqInfo,
    #[error("Failed to serialize the updated APQInfo: {0}")]
    Serialization(tls_codec::Error),
    #[error("The commit carries more than one full APQInfo update.")]
    DuplicateFullUpdate,
    #[error("The commit carries the same APQInfo epoch update more than once.")]
    DuplicateEpochUpdate,
    #[error("The commit mixes a full APQInfo update with epoch updates.")]
    MixedUpdates,
    #[error("The commit updates only one of the two APQInfo epochs.")]
    IncompleteEpochUpdate,
    #[error("The commit both removes and updates the APQInfo.")]
    RemovalWithUpdate,
}

/// The APQInfo updates carried by a single commit.
///
/// The draft allows exactly one of two shapes: a single `full_update`, or a
/// `new_t_epoch` together with a `new_pq_epoch`. Anything else is rejected.
#[derive(Debug, Default)]
pub(super) struct ApqInfoUpdates {
    full: Option<ApqInfoUpdate>,
    t_epoch: Option<ApqInfoUpdate>,
    pq_epoch: Option<ApqInfoUpdate>,
}

impl ApqInfoUpdates {
    /// Whether the commit carries no APQInfo update at all.
    pub(super) fn is_empty(&self) -> bool {
        self.full.is_none() && self.t_epoch.is_none() && self.pq_epoch.is_none()
    }

    /// Records one update, rejecting a second one of the same kind.
    pub(super) fn add(&mut self, update: ApqInfoUpdate) -> Result<(), ApqInfoUpdateError> {
        let (slot, error) = match update {
            ApqInfoUpdate::FullUpdate(_) => {
                (&mut self.full, ApqInfoUpdateError::DuplicateFullUpdate)
            }
            ApqInfoUpdate::NewTEpoch(_) => {
                (&mut self.t_epoch, ApqInfoUpdateError::DuplicateEpochUpdate)
            }
            ApqInfoUpdate::NewPqEpoch(_) => {
                (&mut self.pq_epoch, ApqInfoUpdateError::DuplicateEpochUpdate)
            }
        };
        if slot.replace(update).is_some() {
            return Err(error);
        }
        Ok(())
    }

    /// Applies the recorded updates to the [`ApqInfo`] of the current epoch.
    ///
    /// Returns `None` if the commit carries no APQInfo update at all, which is
    /// the case for a PARTIAL commit.
    pub(super) fn resolve(
        self,
        current: Option<&ApqInfo>,
    ) -> Result<Option<ApqInfo>, ApqInfoUpdateError> {
        let Self {
            full,
            t_epoch,
            pq_epoch,
        } = self;
        match (full, t_epoch, pq_epoch) {
            (None, None, None) => Ok(None),
            (Some(full), None, None) => full.apply(current).map(Some),
            (None, Some(t_epoch), Some(pq_epoch)) => {
                let apq_info = t_epoch.apply(current)?;
                pq_epoch.apply(Some(&apq_info)).map(Some)
            }
            (Some(_), _, _) => Err(ApqInfoUpdateError::MixedUpdates),
            (None, Some(_), None) | (None, None, Some(_)) => {
                Err(ApqInfoUpdateError::IncompleteEpochUpdate)
            }
        }
    }
}

pub(super) fn ensure_extension_support(
    capabilities: Capabilities,
) -> Result<Capabilities, tls_codec::Error> {
    let mut extensions = capabilities.extensions().to_vec();
    if !extensions.contains(&ExtensionType::RequiredCapabilities) {
        extensions.push(ExtensionType::RequiredCapabilities);
    }
    if !extensions.contains(&ExtensionType::AppDataDictionary) {
        extensions.push(ExtensionType::AppDataDictionary);
    }
    let mut proposals: Vec<ProposalType> = capabilities.proposals().to_vec();
    if !proposals.contains(&ProposalType::AppDataUpdate) {
        proposals.push(ProposalType::AppDataUpdate);
    }

    let ciphersuites: Vec<Ciphersuite> = capabilities
        .ciphersuites()
        .iter()
        .map(|&cs| cs.try_into())
        .collect::<Result<_, _>>()?;
    Capabilities::new(
        Some(capabilities.versions()),
        Some(&ciphersuites),
        Some(extensions.as_slice()),
        Some(proposals.as_slice()),
        Some(capabilities.credentials()),
    )
    .pipe(Ok)
}

pub(super) fn ensure_component_support(
    mut dictionary: AppDataDictionary,
) -> Result<AppDataDictionary, tls_codec::Error> {
    let mut app_components: Vec<ComponentId> = dictionary
        .get(&ComponentId::from(ComponentType::AppComponents))
        .map(Vec::tls_deserialize_exact)
        .transpose()?
        .unwrap_or_default();
    if !app_components.contains(&APQMLS_COMPONENT_ID) {
        app_components.push(APQMLS_COMPONENT_ID);
        dictionary.insert(
            ComponentId::from(ComponentType::AppComponents),
            app_components.tls_serialize_detached()?,
        );
    }
    Ok(dictionary)
}

pub(super) fn ensure_leaf_node_component_support(
    mut extensions: Extensions<LeafNode>,
) -> Result<Extensions<LeafNode>, tls_codec::Error> {
    let dictionary = extensions
        .app_data_dictionary()
        .map(|extension| extension.dictionary().clone())
        .unwrap_or_default();
    let dictionary = ensure_component_support(dictionary)?;
    let extension = Extension::AppDataDictionary(AppDataDictionaryExtension::new(dictionary));
    extensions
        .add_or_replace(extension)
        .expect("logic error: extension is valid");
    Ok(extensions)
}

/// Augments the capabilities and extensions of the given leaf node parameters with the support
/// required in an APQMLS group.
pub(super) fn ensure_leaf_node_parameters(
    params: &LeafNodeParameters,
    apq_ciphersuite: ApqCiphersuite,
) -> Result<LeafNodeParameters, tls_codec::Error> {
    let capabilities = params
        .capabilities()
        .cloned()
        .unwrap_or_default()
        .pipe(ensure_extension_support)?
        .pipe(|c| ensure_ciphersuite_support(c, apq_ciphersuite))?;
    let ln_extensions = params
        .extensions()
        .cloned()
        .unwrap_or_default()
        .pipe(ensure_leaf_node_component_support)?;
    let mut builder = LeafNodeParameters::builder()
        .with_capabilities(capabilities)
        .with_extensions(ln_extensions);
    if let Some(credential_with_key) = params.credential_with_key() {
        builder = builder.with_credential_with_key(credential_with_key.clone());
    };
    Ok(builder.build())
}

impl ApqMlsGroup {
    /// Get the APQMLS component from the group, if it exists.
    pub fn apq_info(&self) -> Option<ApqInfo> {
        ApqInfo::from_extensions(self.t_group.extensions()).ok()?
    }
}

impl ApqMlsGroupMut<'_> {
    /// Get the APQMLS component from the group, if it exists.
    pub fn apq_info(&self) -> Option<ApqInfo> {
        ApqInfo::from_extensions(self.t_group.extensions()).ok()?
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn test_apq_info() -> ApqInfo {
        ApqInfo {
            t_session_group_id: GroupId::from_slice(b"t group"),
            pq_session_group_id: GroupId::from_slice(b"pq group"),
            mode: PqtMode::ConfOnly,
            t_cipher_suite: Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519,
            pq_cipher_suite: Ciphersuite::MLS_192_MLKEM1024_AES256GCM_SHA384_P384,
            t_epoch: GroupEpoch::from(3),
            pq_epoch: GroupEpoch::from(4),
        }
    }

    #[test]
    fn full_update_prefixes_the_bare_apq_info() {
        let apq_info = test_apq_info();
        let bare = apq_info.tls_serialize_detached().unwrap();
        let update = ApqInfoUpdate::FullUpdate(apq_info)
            .tls_serialize_detached()
            .unwrap();
        assert_eq!(update[0], 0);
        assert_eq!(&update[1..], bare.as_slice());
    }

    #[test]
    fn epoch_updates_carry_a_u64() {
        let update = ApqInfoUpdate::NewTEpoch(GroupEpoch::from(7))
            .tls_serialize_detached()
            .unwrap();
        assert_eq!(update, [1, 0, 0, 0, 0, 0, 0, 0, 7]);
        let update = ApqInfoUpdate::NewPqEpoch(GroupEpoch::from(7))
            .tls_serialize_detached()
            .unwrap();
        assert_eq!(update, [2, 0, 0, 0, 0, 0, 0, 0, 7]);
    }

    #[test]
    fn update_roundtrip() {
        for update in [
            ApqInfoUpdate::FullUpdate(test_apq_info()),
            ApqInfoUpdate::NewTEpoch(GroupEpoch::from(1)),
            ApqInfoUpdate::NewPqEpoch(GroupEpoch::from(2)),
        ] {
            let bytes = update.tls_serialize_detached().unwrap();
            assert_eq!(
                ApqInfoUpdate::tls_deserialize_exact(&bytes).unwrap(),
                update
            );
        }
    }

    #[test]
    fn unknown_update_type_is_rejected() {
        assert!(ApqInfoUpdate::tls_deserialize_exact([3, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    }

    #[test]
    fn epoch_updates_replace_only_that_epoch() {
        let current = test_apq_info();
        let updated = ApqInfoUpdate::NewTEpoch(GroupEpoch::from(9))
            .apply(Some(&current))
            .unwrap();
        assert_eq!(updated.t_epoch, GroupEpoch::from(9));
        assert_eq!(updated.pq_epoch, current.pq_epoch);
        assert!(updated.matches_except_epochs(&current));

        let updated = ApqInfoUpdate::NewPqEpoch(GroupEpoch::from(9))
            .apply(Some(&current))
            .unwrap();
        assert_eq!(updated.t_epoch, current.t_epoch);
        assert_eq!(updated.pq_epoch, GroupEpoch::from(9));
        assert!(updated.matches_except_epochs(&current));
    }

    #[test]
    fn epoch_update_needs_an_existing_apq_info() {
        assert_eq!(
            ApqInfoUpdate::NewTEpoch(GroupEpoch::from(1)).apply(None),
            Err(ApqInfoUpdateError::NoApqInfo)
        );
        assert_eq!(
            ApqInfoUpdate::NewPqEpoch(GroupEpoch::from(1)).apply(None),
            Err(ApqInfoUpdateError::NoApqInfo)
        );
    }

    #[test]
    fn full_update_needs_no_existing_apq_info() {
        let apq_info = test_apq_info();
        assert_eq!(
            ApqInfoUpdate::FullUpdate(apq_info.clone()).apply(None),
            Ok(apq_info)
        );
    }

    #[test]
    fn only_the_epochs_are_mutable() {
        let apq_info = test_apq_info();
        let tampers: [fn(&mut ApqInfo); 5] = [
            |info| info.mode = PqtMode::ConfAndAuth,
            |info| info.t_session_group_id = GroupId::from_slice(b"other t group"),
            |info| info.pq_session_group_id = GroupId::from_slice(b"other pq group"),
            |info| info.t_cipher_suite = Ciphersuite::MLS_128_DHKEMP256_AES128GCM_SHA256_P256,
            |info| info.pq_cipher_suite = Ciphersuite::MLS_128_MLKEM768_AES256GCM_SHA384_Ed25519,
        ];
        for tamper in tampers {
            let mut tampered = apq_info.clone();
            tamper(&mut tampered);
            assert!(!tampered.matches_except_epochs(&apq_info));
        }

        let mut epochs_changed = apq_info.clone();
        epochs_changed.set_epoch(GroupEpoch::from(41), GroupEpoch::from(42));
        assert!(epochs_changed.matches_except_epochs(&apq_info));
    }
}
