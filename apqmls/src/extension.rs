// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::{
    component::{ComponentData, ComponentId, ComponentType},
    group::{GroupContext, GroupEpoch, GroupId},
    prelude::{
        AppDataDictionary, AppDataDictionaryExtension, Capabilities, Ciphersuite, Extension,
        ExtensionType, Extensions, LeafNode, ProposalType,
    },
};
use tap::Pipe;
use tls_codec::{Deserialize as _, Serialize as _, TlsDeserialize, TlsSerialize, TlsSize};

use crate::{ApqCiphersuite, ApqGroupId, ApqMlsGroup, ApqMlsGroupMut};

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

    pub(super) fn set_epoch(&mut self, t_epoch: GroupEpoch, pq_epoch: GroupEpoch) {
        self.t_epoch = t_epoch;
        self.pq_epoch = pq_epoch;
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
