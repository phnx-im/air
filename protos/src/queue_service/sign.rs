// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::crypto::signatures::keys::{QsClientVerifyingKeyType, QsUserVerifyingKeyType};

use crate::queue_service::v1::{
    CreateClientPayload, CreateClientRequest, DeleteClientPayload, DeleteClientRequest,
    DeleteUserPayload, DeleteUserRequest, InitListenPayload, InitListenRequest,
    PublishKeyPackagesPayload, PublishKeyPackagesRequest, UpdateClientPayload, UpdateClientRequest,
    UpdateUserPayload, UpdateUserRequest,
};
use crate::sign::impl_signed_payload;

impl_signed_payload!(
    UpdateUserRequest,
    UpdateUserPayload,
    QsUserVerifyingKeyType,
    "UpdateUserPayload"
);

impl_signed_payload!(
    DeleteUserRequest,
    DeleteUserPayload,
    QsUserVerifyingKeyType,
    "DeleteUserPayload"
);

impl_signed_payload!(
    CreateClientRequest,
    CreateClientPayload,
    QsUserVerifyingKeyType,
    "CreateClientPayload"
);

impl_signed_payload!(
    UpdateClientRequest,
    UpdateClientPayload,
    QsClientVerifyingKeyType,
    "UpdateClientPayload"
);

impl_signed_payload!(
    DeleteClientRequest,
    DeleteClientPayload,
    QsClientVerifyingKeyType,
    "DeleteClientPayload"
);

impl_signed_payload!(
    PublishKeyPackagesRequest,
    PublishKeyPackagesPayload,
    QsClientVerifyingKeyType,
    "PublishKeyPackagesPayload"
);

impl_signed_payload!(
    InitListenRequest,
    InitListenPayload,
    QsClientVerifyingKeyType,
    "InitListenPayload"
);
