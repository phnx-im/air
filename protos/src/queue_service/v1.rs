// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#![expect(clippy::large_enum_variant)]

tonic::include_proto!("queue_service.v1");

include!(concat!(env!("OUT_DIR"), "/server/queue_service.v1.rs"));
