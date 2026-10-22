// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Groups: lifecycle, membership, deletion, group profile, self updates,
//! resync, epochs and pending operations.

mod deletion;
mod epochs;
mod lifecycle;
mod membership;
mod missed_commits;
mod pending_operations;
mod profile;
mod resync;
mod self_update;

use aircoreclient::SystemMessage;

fn removals(messages: &[SystemMessage]) -> usize {
    messages
        .iter()
        .filter(|message| matches!(message, SystemMessage::Remove(_remover, _removed)))
        .count()
}

fn count(messages: &[SystemMessage], expected: &SystemMessage) -> usize {
    messages
        .iter()
        .filter(|message| *message == expected)
        .count()
}
