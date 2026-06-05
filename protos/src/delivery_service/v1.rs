// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::client_ds::GenerationCollisionDetailTags;

use crate::common::v1::GenerationCollisionDetail;

tonic::include_proto!("delivery_service.v1");

impl GenerationCollisionDetail {
    pub fn tags(&self) -> GenerationCollisionDetailTags {
        self.tags.into()
    }
}
