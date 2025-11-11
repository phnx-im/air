// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/// Find the first error in the source chain that is of type `T`.
pub(crate) fn find_cause<T: std::error::Error + 'static>(
    error: &dyn std::error::Error,
) -> Option<&T> {
    let mut source = error.source();
    while let Some(error) = source {
        if let Some(typed) = error.downcast_ref() {
            return Some(typed);
        }
        source = error.source();
    }
    None
}
