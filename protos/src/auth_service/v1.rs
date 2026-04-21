// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
#![expect(clippy::doc_lazy_continuation)]

use chrono::{DateTime, Days, Months, Utc};
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

    pub fn valid_until_starting_at(&self, at: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            OperationType::Unknown => at,
            OperationType::AddUsername => at + Months::new(1),
            OperationType::GetInviteCode => at + Days::new(1),
        }
    }

    pub fn all() -> impl Iterator<Item = OperationType> {
        Self::VARIANTS
            .iter()
            .filter_map(|v| (*v != Self::Unknown).then_some(*v))
    }
}
