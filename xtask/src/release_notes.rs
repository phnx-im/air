// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Release notes that Google Play and the App Store show as "What's new".
//!
//! Each app version has one file, shared by both stores. The fastlane lanes
//! read the same files (see `app/fastlane/release.rb`).

use std::{fs, io::IsTerminal, process::Command};

use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use xshell::{Shell, cmd};

use crate::candidate::read_answer;

/// Google Play allows 500 characters per language, the App Store 4000.
const MAX_CHARS: usize = 500;

/// Returns the path of the release notes of a version, relative to the
/// workspace root.
pub(crate) fn path(version: &Version) -> String {
    format!("app/stores/common/release_notes/{version}.txt")
}

/// Checks that the release notes are not empty and fit into Google Play.
pub(crate) fn check(notes: &str) -> Result<()> {
    let notes = notes.trim();
    ensure!(!notes.is_empty(), "the release notes are empty");
    let chars = notes.chars().count();
    ensure!(
        chars <= MAX_CHARS,
        "the release notes have {chars} characters, Google Play allows {MAX_CHARS}"
    );
    Ok(())
}

/// Reads and checks the release notes of a version at a git revision.
///
/// Returns None when the revision has no release notes for the version.
pub(crate) fn at_revision(shell: &Shell, rev: &str, version: &Version) -> Result<Option<String>> {
    let path = path(version);
    let spec = format!("{rev}:{path}");
    let Ok(notes) = cmd!(shell, "git show {spec}")
        .quiet()
        .ignore_stderr()
        .read()
    else {
        return Ok(None);
    };
    check(&notes).with_context(|| format!("invalid {path} at {rev}"))?;
    Ok(Some(notes.trim().to_owned()))
}

/// Opens the git editor to write the release notes of a version.
///
/// The draft lives in the temp dir and is reused by the next call. A cut
/// that fails after this step does not lose the text.
pub(crate) fn write_in_editor(shell: &Shell, version: &Version) -> Result<String> {
    if !std::io::stdin().is_terminal() {
        bail!("stdin is not a terminal; pass --release-notes <file>");
    }
    let draft = std::env::temp_dir().join(format!("air-release-notes-{version}.txt"));
    if !draft.exists() {
        fs::write(&draft, template(version))
            .with_context(|| format!("failed to write {}", draft.display()))?;
    }
    // Resolves the editor from GIT_EDITOR, core.editor, VISUAL and EDITOR.
    let editor = cmd!(shell, "git var GIT_EDITOR").quiet().read()?;

    loop {
        // Runs the editor the way git does, which allows arguments in it.
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("{editor} \"$@\""))
            .arg(&editor)
            .arg(&draft)
            .status()
            .with_context(|| format!("failed to run the editor {editor}"))?;
        ensure!(status.success(), "the editor {editor} failed with {status}");

        let content = fs::read_to_string(&draft)
            .with_context(|| format!("failed to read {}", draft.display()))?;
        let notes = strip_comments(&content);
        if notes.is_empty() {
            bail!("the release notes are empty, aborting");
        }
        match check(&notes) {
            Ok(()) => return Ok(notes),
            Err(error) => {
                println!("{error:#}");
                read_answer("Press Enter to edit them again.")?;
            }
        }
    }
}

fn template(version: &Version) -> String {
    format!(
        "\n\
         # Release notes of Air {version}, shown as \"What's new\" in Google Play\n\
         # and the App Store. Write for users, in English, in at most {MAX_CHARS}\n\
         # characters. Lines starting with '#' are dropped. Empty notes abort.\n"
    )
}

fn strip_comments(content: &str) -> String {
    let mut notes = String::new();
    for line in content.lines() {
        if !line.starts_with('#') {
            notes.push_str(line);
            notes.push('\n');
        }
    }
    notes.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_contains_the_full_version() {
        let version = Version::new(0, 27, 1);
        assert_eq!(path(&version), "app/stores/common/release_notes/0.27.1.txt");
    }

    #[test]
    fn check_accepts_notes_up_to_the_limit() {
        assert!(check("Voice messages").is_ok());
        assert!(check(&"a".repeat(MAX_CHARS)).is_ok());
        // Surrounding whitespace is not uploaded.
        assert!(check(&format!("\n{}\n\n", "a".repeat(MAX_CHARS))).is_ok());
        // The limit counts characters, not bytes.
        assert!(check(&"ä".repeat(MAX_CHARS)).is_ok());
    }

    #[test]
    fn check_rejects_empty_and_long_notes() {
        assert!(check("").is_err());
        assert!(check(" \n\t\n").is_err());
        let error = check(&"a".repeat(MAX_CHARS + 1)).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the release notes have 501 characters, Google Play allows 500"
        );
    }

    #[test]
    fn strip_comments_drops_comment_lines_and_outer_blank_lines() {
        let content = "\n- Voice messages\n# a comment\n\n- Faster sync\n\n# trailing\n";
        assert_eq!(strip_comments(content), "- Voice messages\n\n- Faster sync");
        assert_eq!(strip_comments(&template(&Version::new(0, 27, 0))), "");
    }

    #[test]
    fn strip_comments_keeps_hashes_inside_lines() {
        assert_eq!(strip_comments(" #1 fix\nIssue #2"), "#1 fix\nIssue #2");
    }
}
