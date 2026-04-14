// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use prost::Message;
use strum::VariantArray;
use tonic::Status;

tonic::include_proto!("common.v1");

impl StatusDetails {
    pub fn from_status(status: &Status) -> Option<StatusDetails> {
        StatusDetails::decode(status.details()).ok()
    }
}

impl OperationType {
    pub fn max_tokens_allowance(&self) -> u16 {
        match self {
            OperationType::Unknown => 0,
            OperationType::AddUsername => 10,
            OperationType::GetInviteCode => 5,
        }
    }

    pub fn low_tokens_threshold(&self) -> u16 {
        match self {
            OperationType::Unknown => 0,
            OperationType::AddUsername => 5,
            OperationType::GetInviteCode => 1,
        }
    }

    pub fn all() -> impl Iterator<Item = OperationType> {
        Self::VARIANTS
            .into_iter()
            .filter_map(|v| (*v != Self::Unknown).then_some(*v))
    }
}
