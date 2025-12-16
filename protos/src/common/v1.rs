// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use prost::Message;
use tonic::Status;

tonic::include_proto!("common.v1");

impl StatusDetails {
    pub fn from_status(status: &Status) -> Option<StatusDetails> {
        StatusDetails::decode(status.details()).ok()
    }
}
