// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared styling for the progress bars the run displays. Logs go to a file,
//! so these bars are the only thing on the terminal during a run.

use indicatif::ProgressStyle;

/// A determinate bar labelled with `prefix`, ending in the bar's message so
/// `set_message` and `finish_with_message` are actually visible.
pub fn bar_style(prefix: &str) -> ProgressStyle {
    ProgressStyle::with_template(&format!(
        "{{spinner}} {prefix} [{{bar:30.cyan/blue}}] {{pos}}/{{len}} ({{eta}}) {{msg}}"
    ))
    .expect("static progress template is valid")
    .progress_chars("=> ")
}
