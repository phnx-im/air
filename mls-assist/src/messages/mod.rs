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
use thiserror::Error;

#[cfg(doc)]
use openmls::prelude::{PrivateMessage, PublicMessage};

pub mod codec;

#[derive(Debug, Error)]
pub enum AssistedMessageError {
    #[error("Invalid MLSMessage body.")]
    InvalidMessage,
    #[error("Missing group info.")]
    MissingGroupInfo,
}

#[derive(Debug, TlsSerialize, TlsSize)]
pub struct AssistedMessageOut {
    mls_message: MlsMessageOut,
    assisted_group_info_option: Option<AssistedGroupInfo>,
}

impl AssistedMessageOut {
    /// Create a new [`AssistedMessageOut`] from an [`MlsMessageOut`] containing
    /// either a [`PublicMessage`] or a [`PrivateMessage`] and optionally an
    /// [`MlsMessageOut`] containing a [`GroupInfo`].
    ///
    /// Returns an error if the message is a commit and no [`GroupInfo`] is provided.
    pub fn new(
        mls_message: MlsMessageOut,
        group_info_option: Option<MlsMessageOut>,
    ) -> Result<Self, AssistedMessageError> {
        let assisted_group_info_option =
            if let MlsMessageBodyOut::PublicMessage(pub_msg) = mls_message.body() {
                if let Some(MlsMessageBodyOut::GroupInfo(group_info)) =
                    group_info_option.as_ref().map(|m| m.body())
                {
                    Some(AssistedGroupInfo {
                        extensions: group_info.extensions().clone(),
                        signature: group_info.signature().clone(),
                    })
                } else {
                    // If the message is a commit, we require a GroupInfo to be present.
                    if pub_msg.content_type() == ContentType::Commit {
                        return Err(AssistedMessageError::MissingGroupInfo);
                    } else {
                        None
                    }
                }
            } else {
                None
            };
        Ok(Self {
            mls_message,
            assisted_group_info_option,
        })
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
