//! `cargo xtask docs`: what docs.rs builds. docs/sdk.md is `micro-monty`'s
//! crate documentation (`crates/micro-monty/sdk.md` links to it), so its
//! ```` ```rust ```` blocks are the crate's doctests; this runs them and
//! builds the published crates' rustdoc with warnings denied. Blocks that
//! show a type rather than code are fenced ```` ```rust ignore ````. The
//! guide's ```` ```toml ```` dependency snippets are not compiled; every
//! `mod-*` feature they name must be one of `micro-monty`'s.

use crate::{package::PUBLISHED, run_gate, workspace_root};
use std::{env, fs, process::Command};

pub(crate) fn run() -> Result<(), String> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    toml_features(&cargo)?;
    run_gate(
        &cargo,
        &["test", "--doc", "--locked", "--package", "micro-monty"],
    )?;
    let mut args = vec!["doc", "--no-deps", "--locked"];
    for c in PUBLISHED {
        args.extend(["--package", c]);
    }
    println!("check: RUSTDOCFLAGS='-D warnings' cargo {}", args.join(" "));
    let status = Command::new(&cargo)
        .args(&args)
        .env("RUSTDOCFLAGS", "-D warnings")
        .current_dir(workspace_root())
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("gate failed: cargo {}", args.join(" ")))
    }
}

/// Every `"mod-…"` in a ```` ```toml ```` block of docs/sdk.md is a feature
/// of `micro-monty`, as `cargo metadata` reports them.
fn toml_features(cargo: &str) -> Result<(), String> {
    let root = workspace_root();
    let output = Command::new(cargo)
        .args(["metadata", "--no-deps", "--locked", "--format-version", "1"])
        .current_dir(&root)
        .output()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !output.status.success() {
        return Err("cargo metadata failed".to_owned());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("cargo metadata: {error}"))?;
    let features = metadata["packages"]
        .as_array()
        .and_then(|packages| packages.iter().find(|p| p["name"] == "micro-monty"))
        .and_then(|p| p["features"].as_object())
        .ok_or("cargo metadata has no micro-monty features")?;
    let path = root.join("docs/sdk.md");
    let guide = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (mut in_toml, mut named) = (false, 0);
    for (n, line) in guide.lines().enumerate() {
        if line.starts_with("```") {
            in_toml = line == "```toml";
            continue;
        }
        if !in_toml {
            continue;
        }
        for quoted in line.split('"').skip(1).step_by(2) {
            if quoted.starts_with("mod-") {
                if !features.contains_key(quoted) {
                    return Err(format!(
                        "docs/sdk.md:{}: `{quoted}` is not a micro-monty feature",
                        n + 1
                    ));
                }
                named += 1;
            }
        }
    }
    println!("docs: {named} features named in docs/sdk.md's toml snippets exist");
    Ok(())
}
