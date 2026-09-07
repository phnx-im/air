// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use tonic::{Code, Status};

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

pub(crate) trait StatusExt {
    /// Returns true if the error is caused by the client closing the connection.
    fn is_client_disconnect(&self) -> bool;
}

impl StatusExt for Status {
    fn is_client_disconnect(&self) -> bool {
        matches!(self.code(), Code::Unknown)
            && find_cause::<h2::Error>(self)
                .and_then(|h2_error| h2_error.get_io())
                .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::BrokenPipe)
    }
}
