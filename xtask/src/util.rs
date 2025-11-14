use camino::{Utf8Path, Utf8PathBuf};
use once_cell::sync::Lazy;

static WORKSPACE_ROOT: Lazy<Utf8PathBuf> = Lazy::new(|| {
    let manifest_dir = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("xtask is expected to live in the workspace root")
        .to_path_buf()
});

pub fn workspace_root() -> &'static Utf8Path {
    &WORKSPACE_ROOT
}
