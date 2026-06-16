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
    tonic_prost_build::configure()
        .build_client(false)
        .out_dir(&server_dir)
        // Use generated code from the first pass
        .extern_path(".common.v1", "crate::common::v1")
        .extern_path(".auth_service.v1", "crate::auth_service::v1")
        .extern_path(".delivery_service.v1", "crate::delivery_service::v1")
        .extern_path(".queue_service.v1", "crate::queue_service::v1")
        // Override request types containing signed payload
        .extern_path(
            ".auth_service.v1.DeleteUserRequest",
            "crate::signed::SignedRequest<crate::auth_service::v1::DeleteUserRequest, 1, 2>",
        )
        .extern_path(
            ".auth_service.v1.PublishConnectionPackagesRequest",
            "crate::signed::SignedRequest<crate::auth_service::v1::PublishConnectionPackagesRequest, 1, 2>",
        )
        .extern_path(
            ".auth_service.v1.StageUserProfileRequest",
            "crate::signed::SignedRequest<crate::auth_service::v1::StageUserProfileRequest, 1, 2>",
        )
        .extern_path(
            ".auth_service.v1.MergeUserProfileRequest",
            "crate::signed::SignedRequest<crate::auth_service::v1::MergeUserProfileRequest, 1, 2>",
        )
        .extern_path(
            ".auth_service.v1.IssueTokensRequest",
            "crate::signed::SignedRequest<crate::auth_service::v1::IssueTokensRequest, 1, 2>",
        )
        .extern_path(
            ".auth_service.v1.ReportSpamRequest",
            "crate::signed::SignedRequest<crate::auth_service::v1::ReportSpamRequest, 1, 2>",
        )
        .extern_path(
            ".auth_service.v1.CreateUsernameRequest",
            "crate::signed::SignedRequest<crate::auth_service::v1::CreateUsernameRequest, 1, 2>",
        )
        .extern_path(
            ".auth_service.v1.DeleteUsernameRequest",
            "crate::signed::SignedRequest<crate::auth_service::v1::DeleteUsernameRequest, 1, 2>",
        )
        .extern_path(
            ".auth_service.v1.RefreshUsernameRequest",
            "crate::signed::SignedRequest<crate::auth_service::v1::RefreshUsernameRequest, 1, 2>",
        )
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
