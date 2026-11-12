//! `cargo xtask package`: the crates micro-monty publishes package and
//! verify (`cargo package`, each built from its own `.crate` against the
//! others), every package carries LICENSE, THIRD_PARTY.md and the README,
//! and a build from the packages has the workspace build's snapshot build
//! id (docs/spec.md §9.3, HANDOFF scope decision 55).

use crate::{run_gate, workspace_root};
use std::{env, fs, path::Path, process::Command};

/// The published crates, in dependency order; `micro-monty` last.
pub(crate) const PUBLISHED: [&str; 6] = [
    "mm-core",
    "mm-compile",
    "mm-parse",
    "mm-engine",
    "mm-stdlib",
    "micro-monty",
];

/// What every package must contain, beside its sources.
const REQUIRED: [&str; 3] = ["LICENSE", "THIRD_PARTY.md", "README.md"];

pub(crate) fn run() -> Result<(), String> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let root = workspace_root();
    let version = env!("CARGO_PKG_VERSION");
    forget_extracted(version)?;
    // Its own target dir, wiped: cargo reuses a registry crate's build by
    // version, so a verify in `target/` would link earlier packages' code.
    let verify = root.join("target/package-verify");
    let _ = fs::remove_dir_all(&verify);
    let verify = path_str(&verify)?;
    let mut args = vec![
        "package",
        "--locked",
        "--allow-dirty",
        "--target-dir",
        verify.as_str(),
    ];
    for c in PUBLISHED {
        args.extend(["-p", c]);
    }
    run_gate(&cargo, &args)?;
    // Outside the workspace: an unpacked crate's Cargo.toml names none.
    let unpacked = env::temp_dir().join("mm-package-check");
    let _ = fs::remove_dir_all(&unpacked);
    fs::create_dir_all(&unpacked).map_err(|e| format!("{}: {e}", unpacked.display()))?;
    for c in PUBLISHED {
        let file = Path::new(&verify).join(format!("package/{c}-{version}.crate"));
        let status = Command::new("tar")
            .arg("-xzf")
            .arg(&file)
            .arg("-C")
            .arg(&unpacked)
            .status()
            .map_err(|e| format!("cannot run tar: {e}"))?;
        if !status.success() {
            return Err(format!("cannot unpack {}", file.display()));
        }
        let dir = unpacked.join(format!("{c}-{version}"));
        for name in REQUIRED {
            if !dir.join(name).is_file() {
                return Err(format!("the {c} package has no {name}"));
            }
        }
    }
    // The packaged facade against the packaged engine crates.
    let mut patches = Vec::new();
    for c in &PUBLISHED[..PUBLISHED.len() - 1] {
        patches.push("--config".to_owned());
        patches.push(format!(
            "patch.crates-io.{c}.path={:?}",
            path_str(&unpacked.join(format!("{c}-{version}")))?
        ));
    }
    let packaged = build_id(
        &cargo,
        &[
            "--manifest-path".to_owned(),
            path_str(&unpacked.join(format!("micro-monty-{version}/Cargo.toml")))?,
            "--target-dir".to_owned(),
            path_str(&unpacked.join("target"))?,
        ],
        &patches,
    )?;
    let workspace = build_id(
        &cargo,
        &["--package".to_owned(), "micro-monty".to_owned()],
        &[],
    )?;
    if packaged != workspace {
        return Err(format!(
            "the packaged build id {packaged} is not the workspace's {workspace}"
        ));
    }
    println!(
        "package: {} crates packaged, build id {workspace}",
        PUBLISHED.len()
    );
    Ok(())
}

/// The build id `examples/build_id.rs` prints for one way of building.
fn build_id(cargo: &str, place: &[String], config: &[String]) -> Result<String, String> {
    let output = Command::new(cargo)
        .args(["run", "--quiet", "--example", "build_id"])
        .args(place)
        .args(config)
        .current_dir(workspace_root())
        .output()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "the build_id example failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn path_str(p: &Path) -> Result<String, String> {
    p.to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{} is not UTF-8", p.display()))
}

/// Removes cargo's extracted copies of earlier packages of the published
/// crates. `cargo package` verifies each crate against the others through a
/// temporary local registry, which cargo extracts under
/// `$CARGO_HOME/registry/src/-<hash>/` (local registries have no host in
/// the name) and, taking registry crates as immutable, never extracts again
/// for the same version: without this, a verify after a source change
/// builds against the first package's sources.
fn forget_extracted(version: &str) -> Result<(), String> {
    let home = env::var_os("CARGO_HOME").map_or_else(
        || env::var_os("HOME").map(|h| Path::new(&h).join(".cargo")),
        |h| Some(Path::new(&h).to_path_buf()),
    );
    let Some(src) = home.map(|h| h.join("registry/src")) else {
        return Ok(());
    };
    let Ok(entries) = fs::read_dir(&src) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with('-') {
            continue;
        }
        for c in PUBLISHED {
            let dir = entry.path().join(format!("{c}-{version}"));
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
        }
    }
    Ok(())
}
