// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::prelude::tls_codec::{self, TlsDeserialize, TlsSerialize, TlsSize};
use openmls::{
    framing::{ContentType, MlsMessageBodyOut},
    prelude::{
        ConfirmationTag, Extensions, GroupContext, GroupId, KeyPackageRef, LeafNodeIndex,
        MlsMessageOut, ProtocolMessage, Sender, Signature, Welcome,
        group_info::VerifiableGroupInfo,
    },
};

#[cfg(doc)]
use openmls::prelude::{PrivateMessage, PublicMessage, group_info::GroupInfo};

pub mod codec;

#[derive(Debug, TlsSerialize, TlsSize)]
pub struct AssistedMessageOut {
    mls_message: MlsMessageOut,
    assisted_group_info_option: Option<AssistedGroupInfo>,
}

impl AssistedMessageOut {
    /// Create a new [`AssistedMessageOut`] from an [`MlsMessageOut`] containing
    /// either a [`PublicMessage`] or a [`PrivateMessage`] and optionally an
    /// [`MlsMessageOut`] containing a [`GroupInfo`].
    pub fn new(mls_message: MlsMessageOut, group_info_option: Option<MlsMessageOut>) -> Self {
        let is_public_commit = matches!(
            mls_message.body(),
            MlsMessageBodyOut::PublicMessage(pm)
                if pm.content_type() == ContentType::Commit
        );
        let assisted_group_info_option = match group_info_option.as_ref().map(|m| m.body()) {
            Some(MlsMessageBodyOut::GroupInfo(gi)) => {
                // Ensure that GroupInfo is only provided for (public) Commit messages.
                debug_assert!(
                    is_public_commit,
                    "GroupInfo should only be provided for Commit messages."
                );
                Some(AssistedGroupInfo {
                    extensions: gi.extensions().clone(),
                    signature: gi.signature().clone(),
                })
            }
            // Second input should be None or GroupInfo
            Some(_) => {
                debug_assert!(false, "Second input must be GroupInfo if provided.");
                None
            }
            // If no GroupInfo is provided, it must be a non-commit message.
            None => {
                debug_assert!(
                    !is_public_commit,
                    "GroupInfo must be provided for Commit messages."
                );
                None
            }
        };
        Self {
            mls_message,
            assisted_group_info_option,
        }
    }
}

#[derive(Debug)]
pub struct AssistedMessageIn {
    pub(crate) mls_message: ProtocolMessage,
    pub(crate) serialized_mls_message: SerializedMlsMessage,
    pub(crate) group_info_option: Option<AssistedGroupInfoIn>,
}

#[derive(Debug)]
pub struct SerializedMlsMessage(pub Vec<u8>);

impl AssistedMessageIn {
    pub fn into_serialized_mls_message(self) -> SerializedMlsMessage {
        self.serialized_mls_message
    }

    pub fn group_id(&self) -> &GroupId {
        self.mls_message.group_id()
    }

    pub fn sender(&self) -> Option<&Sender> {
        match &self.mls_message {
            ProtocolMessage::PrivateMessage(_) => None,
            ProtocolMessage::PublicMessage(pm) => Some(pm.sender()),
        }
    }
}

#[derive(Debug, TlsSize, Clone, TlsSerialize)]
pub struct AssistedGroupInfo {
    extensions: Extensions,
    signature: Signature,
}

#[derive(Debug, TlsDeserialize, TlsSize, Clone)]
pub struct AssistedGroupInfoIn {
    extensions: Extensions,
    signature: Signature,
}

impl AssistedGroupInfoIn {
    pub fn into_verifiable_group_info(
        self,
        sender_index: LeafNodeIndex,
        group_context: GroupContext,
        confirmation_tag: ConfirmationTag,
    ) -> VerifiableGroupInfo {
        VerifiableGroupInfo::new(
            group_context,
            self.extensions,
            confirmation_tag,
            sender_index,
            self.signature,
        )
    }
}

#[derive(Debug, Clone)]
pub struct AssistedWelcome {
    pub welcome: Welcome,
}

impl AssistedWelcome {
    pub fn joiners(&self) -> impl Iterator<Item = KeyPackageRef> + '_ {
        self.welcome
            .secrets()
            .iter()
            .map(|secret| secret.new_member())
    }
}
