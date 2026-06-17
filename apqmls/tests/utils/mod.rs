// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::ApqMlsGroup;

pub mod client;

pub fn assert_groups_eq(group1: &mut ApqMlsGroup, group2: &mut ApqMlsGroup) {
    let t_group_1_authenticator = group1.t_group.epoch_authenticator();
    let t_group_2_authenticator = group2.t_group.epoch_authenticator();
    assert_eq!(
        t_group_1_authenticator.as_slice(),
        t_group_2_authenticator.as_slice(),
        "t_group secrets do not match"
    );
    let pq_group_1_authenticator = group1.pq_group().epoch_authenticator();
    let pq_group_2_authenticator = group2.pq_group().epoch_authenticator();
    assert_eq!(
        pq_group_1_authenticator.as_slice(),
        pq_group_2_authenticator.as_slice(),
        "pq_group secrets do not match"
    );
}
