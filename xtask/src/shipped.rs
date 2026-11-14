//! How shipped artifacts are built: the shipped cargo invocation, the
//! targets the size budgets apply to, and the `hello` examples, mm-wasm
//! guests and native `mm` built with it (docs/spec.md §6).
//!
//! The size and v8-ir gates key the builds they cache by this file and
//! `wasm.rs` (`size::recipe_key`), so keep here only what decides a
//! shipped artifact's bytes; edits to `check`'s stages in main.rs then
//! leave those caches valid.

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{host_target, micro_monty_binary, wasm, workspace_root};

/// The shipped artifacts' sizes, built from the tree at `src` into the
/// target directory `dir`: `hello` for `wasm32-unknown-unknown` and, on
/// [`SIZE_TARGET_NATIVE`], natively, and both mm-wasm guests.
pub(crate) fn shipped_sizes(src: &Path, dir: &Path) -> Result<Vec<(&'static str, u64)>, String> {
    shipped_artifacts(src, dir)?
        .into_iter()
        .map(|(name, path)| Ok((name, wasm::file_size(&path)?)))
        .collect()
}

/// [`shipped_sizes`]'s artifacts: each one's name and path.
pub(crate) fn shipped_artifacts(
    src: &Path,
    dir: &Path,
) -> Result<Vec<(&'static str, PathBuf)>, String> {
    let wasm = shipped_example_in(src, dir, SIZE_TARGET_WASM, "hello")?;
    let mut measured = vec![("hello.wasm", wasm)];
    let native_target = host_target()?;
    let native = shipped_example_in(src, dir, &native_target, "hello")?;
    if native_target == SIZE_TARGET_NATIVE {
        measured.push(("hello", native));
    } else {
        println!(
            "size: native hello {} bytes on {native_target} (reported; the native gate applies to {SIZE_TARGET_NATIVE})",
            wasm::file_size(&native)?
        );
    }
    measured.extend(shipped_guests(src, dir)?);
    Ok(measured)
}

/// The two mm-wasm guests of [`shipped_artifacts`], built from the tree at
/// `src` into `dir`: each one's name and path.
pub(crate) fn shipped_guests(
    src: &Path,
    dir: &Path,
) -> Result<Vec<(&'static str, PathBuf)>, String> {
    let guest = shipped_guest_in(src, dir)?;
    let bundle = if src == workspace_root() {
        wasm::shipped_bundle_guest(SIZE_TARGET_WASM)?
    } else {
        wasm::bundle_guest_in(src, &dir.join("mm-wasm-bundle"), SIZE_TARGET_WASM)?
    };
    Ok(vec![
        ("mm_wasm_guest.wasm", guest),
        ("mm_wasm_guest_bundle.wasm", bundle),
    ])
}

/// The target of the primary size budget.
const SIZE_TARGET_WASM: &str = "wasm32-unknown-unknown";

/// The host the secondary (native) budget was measured on. Executable
/// formats and code size differ per target (Mach-O on aarch64 macOS, for
/// one), so other hosts report the native size without a budget.
const SIZE_TARGET_NATIVE: &str = "x86_64-unknown-linux-gnu";

/// Builds the facade's example `name` as shipped for `target` and returns
/// its path; on wasm, the module `wasm-opt -Oz` wrote ([`wasm::optimize`]).
/// The example's own crate is compiled at opt 3, as an embedder's binary
/// is: it belongs to the facade's package, whose release profile is `z`,
/// and fat LTO optimizes the whole program at the root crate's level
/// (Cargo.toml).
pub(crate) fn shipped_example(target: &str, name: &str) -> Result<PathBuf, String> {
    let root = workspace_root();
    shipped_example_in(&root, &root.join("target"), target, name)
}

/// [`shipped_example`] for the tree at `src`, built into `dir`.
fn shipped_example_in(src: &Path, dir: &Path, target: &str, name: &str) -> Result<PathBuf, String> {
    let status = shipped_cargo("rustc", target, src)
        .env("CARGO_TARGET_DIR", dir)
        .args(["--package", "micro-monty", "--example", name])
        .args(["--", "-Copt-level=3"])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(format!(
            "shipped release build of {name} for {target} failed"
        ));
    }
    let release = dir.join(format!("{target}/release/examples"));
    if target.starts_with("wasm32") {
        wasm::optimize(&release.join(format!("{name}.wasm")))
    } else {
        Ok(release.join(name))
    }
}

/// Builds the mm-wasm guest (`crates/mm-wasm-guest`) of the tree at `src`
/// as shipped for `wasm32-unknown-unknown` into `dir`, without
/// `test-hooks`, and returns the module `wasm-opt -Oz` wrote.
fn shipped_guest_in(src: &Path, dir: &Path) -> Result<PathBuf, String> {
    let target = SIZE_TARGET_WASM;
    let status = shipped_build_in(target, src)
        .env("CARGO_TARGET_DIR", dir)
        .args(["--package", "mm-wasm-guest"])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(format!(
            "shipped release build of mm-wasm-guest for {target} failed"
        ));
    }
    wasm::optimize(&dir.join(format!("{target}/release/mm_wasm_guest.wasm")))
}

/// Builds `mm` natively as shipped and prints its path,
/// `target/<host>/release/mm`: the binary `bench/callgrind.py` measures by
/// default.
pub(crate) fn shipped_mm() -> Result<(), String> {
    println!("{}", build_shipped_mm()?.display());
    Ok(())
}

/// `mm` built natively as shipped ([`shipped_build`]): its path.
fn build_shipped_mm() -> Result<PathBuf, String> {
    let host = host_target()?;
    let status = shipped_build(&host)
        .args(["--package", "mm-cli", "--bin", "mm"])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(format!("shipped release build of mm for {host} failed"));
    }
    Ok(workspace_root().join(format!("target/{host}/release/mm")))
}

/// The `mm` that measurements time: `$MM` if set, else `mm` built natively
/// as shipped.
pub(crate) fn shipped_mm_binary() -> Result<PathBuf, String> {
    if env::var_os("MM").is_some_and(|value| !value.is_empty()) {
        micro_monty_binary()
    } else {
        build_shipped_mm()
    }
}

/// `cargo xtask shipped-build --target T --package P`: P's binaries built
/// as shipped for T into `target/T/release/`, as release.yml publishes
/// them (owner, 2026-11-13: the published natives are built as shipped).
pub(crate) fn shipped_build_cli(args: &[String]) -> Result<(), String> {
    let (mut target, mut package) = (None, None);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .cloned()
            .ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--target" => target = Some(value?),
            "--package" | "-p" => package = Some(value?),
            other => return Err(format!("shipped-build: unknown option `{other}`")),
        }
    }
    let (Some(target), Some(package)) = (target, package) else {
        return Err("usage: cargo xtask shipped-build --target <triple> --package <name>".into());
    };
    let status = shipped_build(&target)
        .args(["--package", &package, "--bins"])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("shipped build of {package} for {target} failed"))
    }
}

/// `cargo build --release` for `target` the way shipped binaries are built
/// (docs/spec.md §6): std rebuilt with `-Cpanic=immediate-abort` (no panic
/// messages, formatting or backtrace symbolization) and without unwind
/// tables. Both need unstable flags, enabled on the pinned stable toolchain
/// with `RUSTC_BOOTSTRAP=1`; std's source comes from the `rust-src`
/// component (rust-toolchain.toml). The explicit `--target` keeps the flags
/// off build scripts and proc macros. Tests, clippy and other builds use
/// plain stable.
pub(crate) fn shipped_build(target: &str) -> Command {
    shipped_cargo("build", target, &workspace_root())
}

/// [`shipped_build`] in the tree at `src`. Another revision's tree builds
/// with its own toolchain: rustup's `cargo` reads its rust-toolchain.toml,
/// where `$CARGO` is this checkout's toolchain.
pub(crate) fn shipped_build_in(target: &str, src: &Path) -> Command {
    shipped_cargo("build", target, src)
}

/// The `RUSTFLAGS` of shipped builds ([`shipped_build`]).
pub(crate) const SHIPPED_RUSTFLAGS: &str =
    "-Zunstable-options -Cpanic=immediate-abort -Cforce-unwind-tables=no --cfg mm_shipped";

/// [`shipped_build_in`] with another cargo `subcommand` (`rustc`).
fn shipped_cargo(subcommand: &str, target: &str, src: &Path) -> Command {
    let mut command = tree_cargo(src);
    command
        .env("RUSTC_BOOTSTRAP", "1")
        .env("RUSTFLAGS", SHIPPED_RUSTFLAGS)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args([
            subcommand,
            "--release",
            "--locked",
            "-Zbuild-std=std,panic_abort",
        ])
        .args(["--target", target]);
    command
}

/// `cargo` for the tree at `src`, run there: `$CARGO` in this checkout,
/// else rustup's `cargo` without this checkout's toolchain choice, so the
/// tree's own rust-toolchain.toml picks it.
pub(crate) fn tree_cargo(src: &Path) -> Command {
    let mut command = if src == workspace_root() {
        Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
    } else {
        let mut c = Command::new("cargo");
        c.env_remove("RUSTUP_TOOLCHAIN")
            .env_remove("RUSTC")
            .env_remove("RUSTDOC")
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("CARGO_BUILD_TARGET_DIR");
        c
    };
    command.current_dir(src);
    command
}

/// Where the gates check out and build other revisions: outside this
/// checkout, so cargo reads none of its `.cargo/config.toml` there (cargo
/// merges every config file from the working directory up). It is
/// `$MM_REFS_DIR`, else `~/.cache/micro-monty-refs/<checkout>-<hash>`.
pub(crate) fn refs_dir(gate: &str) -> PathBuf {
    let base = env::var_os("MM_REFS_DIR")
        .filter(|v| !v.is_empty())
        .map_or_else(
            || {
                let root = workspace_root();
                let root = root.canonicalize().unwrap_or(root);
                let name = root
                    .file_name()
                    .map_or_default(|n| n.to_string_lossy().into_owned());
                let hash = crate::wasm_ir::source_hash(root.to_string_lossy().as_bytes());
                let cache = env::var_os("XDG_CACHE_HOME")
                    .filter(|v| !v.is_empty())
                    .map_or_else(
                        || PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".cache"),
                        PathBuf::from,
                    );
                cache
                    .join("micro-monty-refs")
                    .join(format!("{name}-{hash}"))
            },
            PathBuf::from,
        );
    base.join(gate)
}
