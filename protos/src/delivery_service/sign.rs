// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::signed::impl_signed_payload;

use aircommon::credentials::keys::ClientKeyType;

impl_signed_payload!(
    request = super::v1::SendMessageRequest,
    payload = super::v1::SendMessagePayload,
    key_type = ClientKeyType,
    label = "SendMessagePayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::WelcomeInfoRequest,
    payload = super::v1::WelcomeInfoPayload,
    key_type = ClientKeyType,
    label = "WelcomeInfoPayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::CreateGroupRequest,
    payload = super::v1::CreateGroupPayload,
    key_type = ClientKeyType,
    label = "CreateGroupPayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::CreateApqGroupRequest,
    payload = super::v1::CreateApqGroupPayload,
    key_type = ClientKeyType,
    label = "CreateApqGroupPayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::GroupOperationRequest,
    payload = super::v1::GroupOperationPayload,
    key_type = ClientKeyType,
    label = "GroupOperationPayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::ApqGroupOperationRequest,
    payload = super::v1::ApqGroupOperationPayload,
    key_type = ClientKeyType,
    label = "ApqGroupOperationPayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::DeleteGroupRequest,
    payload = super::v1::DeleteGroupPayload,
    key_type = ClientKeyType,
    label = "DeleteGroupPayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::TargetedMessageRequest,
    payload = super::v1::TargetedMessagePayload,
    key_type = ClientKeyType,
    label = "TargetedMessagePayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::SelfRemoveRequest,
    payload = super::v1::SelfRemovePayload,
    key_type = ClientKeyType,
    label = "SelfRemovePayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::ResyncRequest,
    payload = super::v1::ResyncPayload,
    key_type = ClientKeyType,
    label = "ResyncPayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::UpdateProfileKeyRequest,
    payload = super::v1::UpdateProfileKeyPayload,
    key_type = ClientKeyType,
    label = "UpdateProfileKeyPayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::ProvisionAttachmentRequest,
    payload = super::v1::ProvisionAttachmentPayload,
    key_type = ClientKeyType,
    label = "ProvisionAttachmentPayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::GetAttachmentUrlRequest,
    payload = super::v1::GetAttachmentUrlPayload,
    key_type = ClientKeyType,
    label = "GetAttachmentPayload",
    seal = private_mod::Seal,
);

impl_signed_payload!(
    request = super::v1::ApqSelfRemoveRequest,
    payload = super::v1::ApqSelfRemovePayload,
    key_type = ClientKeyType,
    label = "ApqSelfRemovePayload",
    seal = private_mod::Seal,
);

mod private_mod {
    #[derive(Default)]
    pub struct Seal;
}
