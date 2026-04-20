// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
#![expect(clippy::doc_lazy_continuation)]

use strum::VariantArray;

tonic::include_proto!("auth_service.v1");

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
            .iter()
            .copied()
            .filter(|&v| v != Self::Unknown)
    }
}
