//! The replay-Ir build recipe, separate from the counting method. The tag
//! reference pins this file and `shipped.rs`, which supplies the Cargo flags.

use std::path::{Path, PathBuf};

pub(crate) fn build(src: &Path, target: &Path, host: &str) -> Result<PathBuf, String> {
    let status = crate::shipped::shipped_build_in(host, src)
        .env(
            "RUSTFLAGS",
            format!("{} --cfg mm_size_wasm", crate::shipped::SHIPPED_RUSTFLAGS),
        )
        .arg("--target-dir")
        .arg(target)
        .args([
            "--quiet",
            "--package",
            "mm-cell-worker",
            "--example",
            "replay_worker",
        ])
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err(format!(
            "shipped build of replay_worker for {host} in {} failed",
            src.display()
        ));
    }
    Ok(target.join(host).join("release/examples/replay_worker"))
}
