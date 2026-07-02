// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fmt,
    path::{Path, PathBuf},
};

use tonic_prost_build::{Config, FileDescriptorSet};

const PROTOS: &[&str] = &[
    "api/auth_service/v1/auth_service.proto",
    "api/delivery_service/v1/delivery_service.proto",
    "api/queue_service/v1/queue_service.proto",
];

/// Name of the field whose raw wire bytes are signed and verified in a `SignedRequest`.
const SIGNED_FIELD: &str = "payload";

fn config(protoc_path: &Path) -> Config {
    let mut config = Config::new();
    config.protoc_executable(protoc_path).enum_attribute(
        "auth_service.v1.OperationType",
        "#[derive(strum::VariantArray, strum::Display)]",
    );
    config
}

fn main() {
    let protoc_path = protoc_bin_vendored::protoc_bin_path().unwrap();

    // Pass 1: messages + clients
    tonic_prost_build::configure()
        .build_server(false)
        .compile_with_config(config(&protoc_path), PROTOS, &["api"])
        .unwrap();

    let fds = config(&protoc_path).load_fds(PROTOS, &["api"]).unwrap();

    // Pass 2: servers
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let server_dir = out_dir.join("server");
    std::fs::create_dir_all(&server_dir).unwrap();
    let mut builder = tonic_prost_build::configure()
        .build_client(false)
        .out_dir(&server_dir)
        // Use generated code from the first pass
        .extern_path(".common.v1", "crate::common::v1")
        .extern_path(".auth_service.v1", "crate::auth_service::v1")
        .extern_path(".delivery_service.v1", "crate::delivery_service::v1")
        .extern_path(".queue_service.v1", "crate::queue_service::v1");
    for (service, request_type) in SIGNED_REQUESTS {
        let package = format!("{service}.v1");
        let tag = payload_field_number(&fds, &package, request_type).unwrap_or_else(|| {
            panic!(
                "signed request {package}.{request_type} has no `{SIGNED_FIELD}` field; \
                 fix the proto or remove it from SIGNED_REQUESTS",
            )
        });
        builder = builder.extern_path(
            format!(".{package}.{request_type}"),
            format!("crate::signed::SignedRequest<crate::{service}::v1::{request_type}, {tag}>",),
        );
    }
    builder
        .compile_with_config(config(&protoc_path), PROTOS, &["api"])
        .unwrap();

    let mut relay_config = Config::new();
    relay_config
        .protoc_executable(protoc_path)
        .bytes([".relay_service.v1.RelayFrame.payload"]);
    tonic_prost_build::configure()
        .codec_path("crate::relay_service::codec::BytesCodec")
        .compile_with_config(
            relay_config,
            &["api/relay_service/v1/relay_service.proto"],
            &["api"],
        )
        .unwrap();

    println!("cargo:rerun-if-changed=api");
}

/// Requests that should be wrapped in `SignedRequest<T>`.
///
/// When decoding protobuf bytes, the payload and signature are extracted as bytes and stored in the
/// wrapper, which allows verifying the payload without encoding it again. The payload field number
/// (the `TAG` of `SignedRequest`) is derived from the proto at build time, see
/// [`payload_field_number`].
const SIGNED_REQUESTS: &[(Service, &str)] = &[
    // As
    (Service::As, "DeleteUserRequest"),
    (Service::As, "PublishConnectionPackagesRequest"),
    (Service::As, "StageUserProfileRequest"),
    (Service::As, "MergeUserProfileRequest"),
    (Service::As, "IssueTokensRequest"),
    (Service::As, "ReportSpamRequest"),
    (Service::As, "CreateUsernameRequest"),
    (Service::As, "DeleteUsernameRequest"),
    (Service::As, "RefreshUsernameRequest"),
    // Ds
    (Service::Ds, "SendMessageRequest"),
    (Service::Ds, "WelcomeInfoRequest"),
    (Service::Ds, "CreateGroupRequest"),
    (Service::Ds, "CreateApqGroupRequest"),
    (Service::Ds, "GroupOperationRequest"),
    (Service::Ds, "ApqGroupOperationRequest"),
    (Service::Ds, "DeleteGroupRequest"),
    (Service::Ds, "ApqDeleteGroupRequest"),
    (Service::Ds, "TargetedMessageRequest"),
    (Service::Ds, "SelfRemoveRequest"),
    (Service::Ds, "ApqSelfRemoveRequest"),
    (Service::Ds, "ResyncRequest"),
    (Service::Ds, "UpdateProfileKeyRequest"),
    (Service::Ds, "ProvisionAttachmentRequest"),
    (Service::Ds, "GetAttachmentUrlRequest"),
    // Qs
    (Service::Qs, "UpdateUserRequest"),
    (Service::Qs, "DeleteUserRequest"),
    (Service::Qs, "CreateClientRequest"),
    (Service::Qs, "UpdateClientRequest"),
    (Service::Qs, "DeleteClientRequest"),
    (Service::Qs, "PublishKeyPackagesRequest"),
    (Service::Qs, "PublishApqKeyPackagesRequest"),
];

enum Service {
    As,
    Ds,
    Qs,
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Service::As => f.write_str("auth_service"),
            Service::Ds => f.write_str("delivery_service"),
            Service::Qs => f.write_str("queue_service"),
        }
    }
}

/// Look up the field number of the `payload` field of `package.message` in the descriptor set.
///
/// Returns `None` if the message has no such field, which the caller turns into a build error.
fn payload_field_number(fds: &FileDescriptorSet, package: &str, message: &str) -> Option<u32> {
    fds.file
        .iter()
        .filter(|file| file.package() == package)
        .flat_map(|file| &file.message_type)
        .find(|msg| msg.name() == message)?
        .field
        .iter()
        .find(|field| field.name() == SIGNED_FIELD)
        .map(|field| field.number() as u32)
}
