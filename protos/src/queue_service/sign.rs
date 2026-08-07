// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::crypto::signatures::keys::{QsClientVerifyingKeyType, QsUserVerifyingKeyType};

use crate::signed::impl_signed_payload;

impl_signed_payload!(
    request = super::v1::UpdateUserRequest,
    payload = super::v1::UpdateUserPayload,
    key_type = QsUserVerifyingKeyType,
    label = "UpdateUserPayload",
    seal = private::Seal,
);

impl_signed_payload!(
    request = super::v1::DeleteUserRequest,
    payload = super::v1::DeleteUserPayload,
    key_type = QsUserVerifyingKeyType,
    label = "DeleteUserPayload",
    seal = private::Seal,
);

impl_signed_payload!(
    request = super::v1::CreateClientRequest,
    payload = super::v1::CreateClientPayload,
    key_type = QsUserVerifyingKeyType,
    label = "CreateClientPayload",
    seal = private::Seal,
);

impl_signed_payload!(
    request = super::v1::UpdateClientRequest,
    payload = super::v1::UpdateClientPayload,
    key_type = QsClientVerifyingKeyType,
    label = "UpdateClientPayload",
    seal = private::Seal,
);

impl_signed_payload!(
    request = super::v1::DeleteClientRequest,
    payload = super::v1::DeleteClientPayload,
    key_type = QsClientVerifyingKeyType,
    label = "DeleteClientPayload",
    seal = private::Seal,
);

impl_signed_payload!(
    request = super::v1::StageKeyPackagesRequest,
    payload = super::v1::StageKeyPackagesPayload,
    key_type = QsClientVerifyingKeyType,
    label = "StageKeyPackagesPayload",
    seal = private::Seal,
);

impl_signed_payload!(
    request = super::v1::PublishKeyPackagesRequest,
    payload = super::v1::PublishKeyPackagesPayload,
    key_type = QsClientVerifyingKeyType,
    label = "PublishKeyPackagesPayload",
    seal = private::Seal,
);

impl_signed_payload!(
    request = super::v1::PublishApqKeyPackagesRequest,
    payload = super::v1::PublishApqKeyPackagesPayload,
    key_type = QsClientVerifyingKeyType,
    label = "PublishApqKeyPackagesRequest",
    seal = private::Seal,
);

impl_signed_payload!(
    request = super::v1::InitListenRequest,
    payload = super::v1::InitListenPayload,
    key_type = QsClientVerifyingKeyType,
    label = "InitListenPayload",
    seal = private::Seal,
);

mod private {
    #[derive(Default)]
    pub struct Seal;
}
