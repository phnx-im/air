// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Compatibility checks for add candidates.
//!
//! Checking candidates upfront allows leaving incompatible ones out of the
//! commit instead of failing the whole commit.

use anyhow::Context;
use openmls::prelude::ProposalValidationError;

use crate::contacts::ContactKeyPackage;

use super::Group;

impl Group {
    /// Checks whether an invitee's key package is compatible with this group.
    ///
    /// Compatible means that the key package passes the OpenMLS validation
    /// that an add commit performs on it, both locally when the commit is
    /// built and on the DS when it is processed.
    pub(crate) fn check_invitee_compatibility(
        &self,
        key_package: &ContactKeyPackage,
    ) -> anyhow::Result<Result<(), ProposalValidationError>> {
        match key_package {
            ContactKeyPackage::Traditional(key_package) => Ok(self
                .mls_group
                .public_group()
                .validate_key_package_for_add(key_package)),
            ContactKeyPackage::Apq(key_package) => {
                let pq = self.pq.as_ref().context("No PQ group found")?;
                let t_result = self
                    .mls_group
                    .public_group()
                    .validate_key_package_for_add(key_package.t_key_package());
                if t_result.is_err() {
                    return Ok(t_result);
                }
                Ok(pq
                    .mls_group
                    .public_group()
                    .validate_key_package_for_add(key_package.pq_key_package()))
            }
        }
    }
}
