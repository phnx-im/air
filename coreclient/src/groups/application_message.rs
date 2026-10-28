// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{LeafCredential, keys::UserSigningKey},
    identifiers::UserId,
    messages::client_ds_out::{
        CollisionTag, SendMessageCollisionTag, SendMessageParamsOut, TargetedMessageParamsOut,
        TargetedMessageType,
    },
};
use anyhow::Result;
use hkdf::Hkdf;
use mimi_content::{MessageStatus, MessageStatusReport, MimiContent, PerMessageStatus};
use mls_assist::messages::AssistedMessageOut;
use openmls::{
    components::vc_derivation_info::GenerationId,
    group::{ExportSecretError, GroupEpoch, UnconfirmedMessage},
    prelude::{OpenMlsProvider, tls_codec::Serialize as TlsSerializeTrait},
};
use openmls_traits::signatures::Signer;
use sha2::Sha256;

use crate::{
    clients::{attachment::MimiContentExt, targeted_message::TargetedMessageContent},
    groups::{
        Group,
        error::{GroupOperationError, TargetedMessageError},
        openmls_provider::AirOpenMlsProvider,
    },
};

#[derive(Debug)]
pub(super) struct SendMessageCollisionKey {
    // The group epoch this secret key was exported from.
    epoch: GroupEpoch,
    // The HKDF instance used to derive the collision tag.
    kdf: Hkdf<Sha256>,
}

impl SendMessageCollisionKey {
    pub fn try_from_group(
        group: &mut Group,
        provider: &AirOpenMlsProvider,
    ) -> Result<Self, ExportSecretError> {
        let salt = group.own_index().u32().to_le_bytes();
        let epoch = group.mls_group.epoch();
        let epoch_secret = group.mls_group.export_secret(
            provider.crypto(),
            "virtual-client-collision-detection-v1",
            &salt,
            32,
        )?;

        let kdf =
            Hkdf::from_prk(epoch_secret.as_slice()).expect("input is 32 bytes, a valid HKDF PRK");
        Ok(Self { epoch, kdf })
    }

    fn derive_collision_tag(&self, prefix: &'static str, info: &[u8]) -> CollisionTag {
        // our KDF is using SHA-256 but you can request less bytes, it is just truncated internally.
        let mut tag: [u8; 8] = [0u8; 8];
        let info = &[prefix.as_bytes(), info].concat();
        self.kdf
            .expand(info.as_slice(), &mut tag)
            .expect("8 bytes is a valid HKDF-Expand SHA-256 truncated output length");
        CollisionTag::new(i64::from_le_bytes(tag))
    }
}

trait GenerateCollisionTag {
    fn collision_tag(&self, key: &SendMessageCollisionKey) -> SendMessageCollisionTag;
}

fn generation_id_to_collision_tag(generation_id: &GenerationId) -> CollisionTag {
    let mut buf = [0u8; 8];
    let slice = generation_id.as_slice();
    let n = slice.len().min(8);
    buf[..n].copy_from_slice(&slice[..n]);
    CollisionTag::new(i64::from_le_bytes(buf))
}

impl GenerateCollisionTag for PerMessageStatus {
    fn collision_tag(&self, key: &SendMessageCollisionKey) -> SendMessageCollisionTag {
        match self.status {
            MessageStatus::Delivered => SendMessageCollisionTag::DeliveryReceipt(
                key.derive_collision_tag("delivered", &self.mimi_id),
            ),
            MessageStatus::Read => SendMessageCollisionTag::ReadReceipt(
                key.derive_collision_tag("read", &self.mimi_id),
            ),
            _ => SendMessageCollisionTag::Other(key.derive_collision_tag("aux", &self.mimi_id)),
        }
    }
}

impl Group {
    /// Derive and register the collision-detection key for the current epoch if not already set
    pub(crate) fn ensure_collision_key(
        &mut self,
        provider: &AirOpenMlsProvider,
    ) -> Result<(), ExportSecretError> {
        if let Some(key) = self.send_message_collision_key.as_ref()
            && key.epoch == self.mls_group.epoch()
        {
            return Ok(());
        }

        self.send_message_collision_key =
            Some(SendMessageCollisionKey::try_from_group(self, provider)?);
        Ok(())
    }

    /// Send an application message to the group.
    ///
    /// Collision tags are only included when a collision key has been registered via
    /// `ensure_collision_key` (i.e. the group is acting as a virtual client).
    pub(crate) fn create_message(
        &mut self,
        provider: &AirOpenMlsProvider<'_>,
        signer: &impl Signer,
        content: MimiContent,
        message_status_report: Option<MessageStatusReport>,
    ) -> Result<SendMessageParamsOut, GroupOperationError> {
        let UnconfirmedMessage {
            message,
            epoch,
            generation,
            generation_id,
        } = self
            .mls_group
            .create_unconfirmed_message(provider, signer, &content.serialize()?)?;

        let mut collision_tags = Vec::new();
        if let Some(generation_id) = generation_id {
            collision_tags.push(SendMessageCollisionTag::Generation(
                generation_id_to_collision_tag(&generation_id),
            ));

            if let Some(message_status_report) = message_status_report {
                self.ensure_collision_key(provider)?;
                if let Some(key) = self.send_message_collision_key.as_ref() {
                    for pms in message_status_report.statuses {
                        collision_tags.push(pms.collision_tag(key));
                    }
                }
            }
        }

        let message = AssistedMessageOut::new(message, None);
        let suppress_notifications = suppress_notifications(&content);

        let send_message_params = SendMessageParamsOut {
            sender: self.mls_group.own_leaf_index(),
            message,
            suppress_notifications,
            epoch,
            generation,
            collision_tags,
        };

        Ok(send_message_params)
    }

    /// Send an application message to the group.
    pub(crate) fn create_targeted_application_message(
        &mut self,
        provider: &AirOpenMlsProvider<'_>,
        signer: &UserSigningKey,
        recipient: UserId,
        content: TargetedMessageContent,
    ) -> Result<TargetedMessageParamsOut, GroupOperationError> {
        let content_bytes = content.tls_serialize_detached()?;
        let UnconfirmedMessage {
            message,
            generation,
            generation_id,
            epoch: _,
        } = self
            .mls_group
            .create_unconfirmed_message(provider, signer, &content_bytes)?;

        let mut collision_tags = Vec::new();
        if let Some(generation_id) = generation_id {
            collision_tags.push(SendMessageCollisionTag::Generation(
                generation_id_to_collision_tag(&generation_id),
            ));
        }

        let message = AssistedMessageOut::new(message, None);

        let recipient_index = self
            .mls_group()
            .members()
            .find_map(|m| {
                let user_credential = LeafCredential::from_credential(&m.credential).ok()?;
                if user_credential.user_id(self.own_user_id()) == &recipient {
                    Some(m.index)
                } else {
                    None
                }
            })
            .ok_or(TargetedMessageError::RecipientNotInGroup)?;

        let params = TargetedMessageParamsOut {
            sender: self.mls_group.own_leaf_index(),
            generation,
            collision_tags,
            message_type: TargetedMessageType::ApplicationMessage {
                message,
                recipient: recipient_index,
            },
        };

        Ok(params)
    }

    /// Mark the message sent at this generation as confirmed (accepted by DS).
    pub(crate) fn confirm_application_message(
        &mut self,
        provider: &AirOpenMlsProvider<'_>,
        epoch: GroupEpoch,
        generation: u32,
    ) -> Result<(), GroupOperationError> {
        self.mls_group
            .confirm_application_message(provider.storage(), epoch, generation)
            .map_err(Into::into)
    }
}

/// Returns true if the QS should suppress notifications for this message.
pub fn suppress_notifications(content: &MimiContent) -> bool {
    if content.is_status_update() {
        // Status updates should never trigger notifications.
        return true;
    }
    if content.self_group_message().is_some() {
        // Bookkeeping among the user's own devices, never shown.
        return true;
    }
    if content.replaces.is_some() {
        // Replaces indicates an edit or a deletion, which should not
        // trigger notifications.
        return true;
    }
    // All other messages should trigger notifications.
    false
}
