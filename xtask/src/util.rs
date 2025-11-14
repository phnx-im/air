// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::LazyLock;

use camino::{Utf8Path, Utf8PathBuf};

static WORKSPACE_ROOT: LazyLock<Utf8PathBuf> = LazyLock::new(|| {
    let manifest_dir = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("xtask is expected to live in the workspace root")
        .to_path_buf()
});

pub fn workspace_root() -> &'static Utf8Path {
    &WORKSPACE_ROOT
}
