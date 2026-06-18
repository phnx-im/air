// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::PublicGroup;

/// An APQMLS public group, consisting of a traditional public group and a post-quantum public
/// group.
///
/// The two groups can be used independently, except for membership updates.
#[derive(Debug)]
pub struct ApqPublicGroup {
    t_public_group: PublicGroup,
    pq_public_group: PublicGroup,
}

/// Same as [`ApqPublicGroup`], but references public groups instead of owning them.
#[derive(Debug)]
pub struct ApqPublicGroupMut<'a> {
    pub(crate) t_public_group: &'a mut PublicGroup,
    pub(crate) pq_public_group: &'a mut PublicGroup,
}

impl ApqPublicGroup {
    /// Create a new APQMLS public group from the traditional and post-quantum MLS public groups.
    pub fn from_groups(t_public_group: PublicGroup, pq_public_group: PublicGroup) -> Self {
        Self {
            t_public_group,
            pq_public_group,
        }
    }

    pub fn as_mut(&mut self) -> ApqPublicGroupMut<'_> {
        ApqPublicGroupMut::from_groups(&mut self.t_public_group, &mut self.pq_public_group)
    }
}

impl<'a> ApqPublicGroupMut<'a> {
    /// A non-owning version of [`ApqPublicGroup::from_groups`].
    pub fn from_groups(
        t_public_group: &'a mut PublicGroup,
        pq_public_group: &'a mut PublicGroup,
    ) -> Self {
        Self {
            t_public_group,
            pq_public_group,
        }
    }

    pub fn t_public_group(&mut self) -> &mut PublicGroup {
        self.t_public_group
    }

    pub fn pq_public_group(&mut self) -> &mut PublicGroup {
        self.pq_public_group
    }
}
