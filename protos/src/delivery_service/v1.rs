// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use enumset::{EnumSet, EnumSetType};

use crate::common::v1::GenerationCollisionDetail;

tonic::include_proto!("delivery_service.v1");

#[derive(Debug, Clone, Default)]
pub struct GenerationCollisionDetailTags(pub(crate) EnumSet<GenerationCollisionDetailTag>);

impl GenerationCollisionDetailTags {
    pub fn insert(&mut self, tag: GenerationCollisionDetailTag) {
        self.0.insert(tag);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, other: GenerationCollisionDetailTag) -> bool {
        self.0.contains(other)
    }
}

#[derive(Debug, EnumSetType)]
pub enum GenerationCollisionDetailTag {
    Tag1,
    Tag2,
}

impl GenerationCollisionDetail {
    pub fn tags(&self) -> GenerationCollisionDetailTags {
        GenerationCollisionDetailTags(EnumSet::from_u32_truncated(self.tags))
    }
}
