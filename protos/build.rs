// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

use tonic_prost_build::Config;

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
        .compile_with_config(
            config(&protoc_path),
            &[
                "api/auth_service/v1/auth_service.proto",
                "api/delivery_service/v1/delivery_service.proto",
                "api/queue_service/v1/queue_service.proto",
            ],
            &["api"],
        )
        .unwrap();

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
    for config in SIGNED_REQUEST_CONFIGS {
        builder = builder.extern_path(
            format!(".{}.v1.{}", config.service.as_str(), config.request_type),
            format!(
                "crate::signed::SignedRequest<crate::{}::v1::{}, {}, {}>",
                config.service.as_str(),
                config.request_type,
                config.payload_tag,
                config.signature_tag
            ),
        );
    }
    builder
        .compile_with_config(
            config(&protoc_path),
            &[
                "api/auth_service/v1/auth_service.proto",
                "api/delivery_service/v1/delivery_service.proto",
                "api/queue_service/v1/queue_service.proto",
            ],
            &["api"],
        )
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

/// Requests that should be wrapped in `SignedRequest<T>`
///
/// When deconding protobuf bytes, the payload and signature will be extracted as bytes and stored
/// in the wrapper. Allows to verify payload without encoding the payload again.
const SIGNED_REQUEST_CONFIGS: &[SignedRequestConfig] = &[
    // As
    sr(Service::As, "DeleteUserRequest"),
    sr(Service::As, "PublishConnectionPackagesRequest"),
    sr(Service::As, "StageUserProfileRequest"),
    sr(Service::As, "MergeUserProfileRequest"),
    sr(Service::As, "IssueTokensRequest"),
    sr(Service::As, "ReportSpamRequest"),
    sr(Service::As, "CreateUsernameRequest"),
    sr(Service::As, "DeleteUsernameRequest"),
    sr(Service::As, "RefreshUsernameRequest"),
    // Ds
    sr(Service::Ds, "SendMessageRequest"),
    srt(Service::Ds, "WelcomeInfoRequest", 2, 1),
    sr(Service::Ds, "CreateGroupRequest"),
    sr(Service::Ds, "CreateApqGroupRequest"),
    sr(Service::Ds, "GroupOperationRequest"),
    sr(Service::Ds, "ApqGroupOperationRequest"),
    sr(Service::Ds, "DeleteGroupRequest"),
    sr(Service::Ds, "TargetedMessageRequest"),
    srt(Service::Ds, "SelfRemoveRequest", 2, 1),
    sr(Service::Ds, "ResyncRequest"),
    sr(Service::Ds, "UpdateProfileKeyRequest"),
    sr(Service::Ds, "ProvisionAttachmentRequest"),
    sr(Service::Ds, "GetAttachmentUrlRequest"),
    // Qs
    srt(Service::Qs, "UpdateUserRequest", 5, 6),
    srt(Service::Qs, "DeleteUserRequest", 3, 4),
    srt(Service::Qs, "CreateClientRequest", 7, 8),
    srt(Service::Qs, "UpdateClientRequest", 6, 7),
    srt(Service::Qs, "DeleteClientRequest", 3, 4),
    srt(Service::Qs, "PublishKeyPackagesRequest", 4, 5),
    sr(Service::Qs, "PublishApqKeyPackagesRequest"),
];

/// Construct a `SignedRequestConfig` with default payload and signature tags (1, 2)
const fn sr(service: Service, request_type: &'static str) -> SignedRequestConfig {
    SignedRequestConfig {
        service,
        request_type,
        payload_tag: 1,
        signature_tag: 2,
    }
}

/// Construct a `SignedRequestConfig` with custom payload and signature tags
const fn srt(
    service: Service,
    request_type: &'static str,
    payload_tag: u32,
    signature_tag: u32,
) -> SignedRequestConfig {
    SignedRequestConfig {
        service,
        request_type,
        payload_tag,
        signature_tag,
    }
}

enum Service {
    As,
    Ds,
    Qs,
}

impl Service {
    fn as_str(&self) -> &'static str {
        match self {
            Service::As => "auth_service",
            Service::Ds => "delivery_service",
            Service::Qs => "queue_service",
        }
    }
}

struct SignedRequestConfig {
    service: Service,
    request_type: &'static str,
    payload_tag: u32,
    signature_tag: u32,
}
