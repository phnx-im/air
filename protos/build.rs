// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use tonic_build::Config;

fn main() {
    let protoc_path = protoc_bin_vendored::protoc_bin_path().unwrap();
    let mut config = Config::new();
    config.protoc_executable(protoc_path);
    tonic_build::configure()
        .compile_protos_with_config(
            config,
            &[
                "api/auth_service/v1/auth_service.proto",
                "api/delivery_service/v1/delivery_service.proto",
                "api/queue_service/v1/queue_service.proto",
            ],
            &["api"],
        )
        .unwrap();
}
