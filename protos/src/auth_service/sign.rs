// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::credentials::keys::{self, ClientKeyType, UsernameKeyType};

use crate::signed::impl_signed_payload2;

impl_signed_payload2!(
    request = super::v1::DeleteUserRequest,
    payload = super::v1::DeleteUserPayload,
    key_type = ClientKeyType,
    label = "DeleteUserPayload",
    seal = private_mod::Seal,
);

impl_signed_payload2!(
    request = super::v1::PublishConnectionPackagesRequest,
    payload = super::v1::PublishConnectionPackagesPayload,
    key_type = UsernameKeyType,
    label = "PublishConnectionPackagesPayload",
    seal = private_mod::Seal,
);

impl_signed_payload2!(
    request = super::v1::StageUserProfileRequest,
    payload = super::v1::StageUserProfilePayload,
    key_type = ClientKeyType,
    label = "StageUserProfilePayload",
    seal = private_mod::Seal,
);

impl_signed_payload2!(
    request = super::v1::MergeUserProfileRequest,
    payload = super::v1::MergeUserProfilePayload,
    key_type = ClientKeyType,
    label = "MergeUserProfilePayload",
    seal = private_mod::Seal,
);

impl_signed_payload2!(
    request = super::v1::IssueTokensRequest,
    payload = super::v1::IssueTokensPayload,
    key_type = ClientKeyType,
    label = "IssueTokensPayload",
    seal = private_mod::Seal,
);

impl_signed_payload2!(
    request = super::v1::ReportSpamRequest,
    payload = super::v1::ReportSpamPayload,
    key_type = ClientKeyType,
    label = "ReportSpamPayload",
    seal = private_mod::Seal,
);

impl_signed_payload2!(
    request = super::v1::CreateUsernameRequest,
    payload = super::v1::CreateUsernamePayload,
    key_type = keys::UsernameKeyType,
    label = "CreateHandlePayload",
    seal = private_mod::Seal,
);

impl_signed_payload2!(
    request = super::v1::DeleteUsernameRequest,
    payload = super::v1::DeleteUsernamePayload,
    key_type = keys::UsernameKeyType,
    label = "DeleteHandlePayload",
    seal = private_mod::Seal,
);

impl_signed_payload2!(
    request = super::v1::RefreshUsernameRequest,
    payload = super::v1::RefreshUsernamePayload,
    key_type = keys::UsernameKeyType,
    label = "RefreshHandlePayload",
    seal = private_mod::Seal,
);

impl_signed_payload2!(
    request = super::v1::InitListenUsernameRequest,
    payload = super::v1::InitListenUsernamePayload,
    key_type = keys::UsernameKeyType,
    label = "InitListenHandleRequest",
    seal = private_mod::Seal,
);

mod private_mod {
    #[derive(Default)]
    pub struct Seal;
}
