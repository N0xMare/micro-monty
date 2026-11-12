//! `cargo xtask wasm`: the facade on wasm (docs/spec.md §10.3).
//!
//! Builds `micro-monty` and its examples as shipped
//! ([`crate::shipped_build`]) for `wasm32-wasip1` and
//! `wasm32-unknown-unknown`, runs binaryen's `wasm-opt -Oz` on the `hello`
//! and `roundtrip` examples ([`optimize`]), reports `hello`'s size on each
//! before and after, and, when wasmtime is found (`$WASMTIME`,
//! `~/.local/bin/wasmtime`, else `wasmtime` on `PATH`), runs the optimized
//! `roundtrip` example on both: a cell parked on fuel, snapshotted,
//! restored into a new session and finished. On `wasm32-unknown-unknown`
//! std has no clock or randomness, so the example provides them as an
//! embedder would; it runs through its exported `main`. There it also runs
//! `crates/mm-wasm-alloc`'s `realloc` example, the shipped allocator's
//! over-aligned `realloc`.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::workspace_root;

const TARGETS: [&str; 2] = ["wasm32-wasip1", "wasm32-unknown-unknown"];

/// The features rustc 1.99 enables by default on both wasm targets
/// (`rustc --print cfg --target wasm32-unknown-unknown`, plus the two
/// LLVM implies). The stripped module carries no `target_features`
/// section, so wasm-opt is told; it must not use any other feature.
const WASM_FEATURES: [&str; 8] = [
    "--enable-bulk-memory",
    "--enable-bulk-memory-opt",
    "--enable-call-indirect-overlong",
    "--enable-multivalue",
    "--enable-mutable-globals",
    "--enable-nontrapping-float-to-int",
    "--enable-reference-types",
    "--enable-sign-ext",
];

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    if !args.is_empty() {
        return Err("usage: cargo xtask wasm".to_owned());
    }
    let root = workspace_root();
    for target in TARGETS {
        let status = crate::shipped_build(target)
            .args(["--package", "micro-monty", "--examples"])
            .status()
            .map_err(|error| format!("cannot run cargo: {error}"))?;
        if !status.success() {
            return Err(format!(
                "release build for {target} failed (is the target installed? \
                 `rustup target add {target}`)"
            ));
        }
        optimize(&example(&root, target, "roundtrip"))?;
        // `hello` as `cargo xtask size` builds it (its own crate at opt 3).
        let shipped = crate::shipped_example(target, "hello")?;
        let hello = example(&root, target, "hello");
        println!(
            "wasm: {target} hello.wasm {} bytes, {} after wasm-opt -Oz",
            file_size(&hello)?,
            file_size(&shipped)?
        );
    }
    let Some(wasmtime) = tool("WASMTIME", "wasmtime") else {
        println!("wasm: wasmtime not found ($WASMTIME, ~/.local/bin, PATH); not run");
        return Ok(());
    };
    let wasi = shipped_path(&example(&root, "wasm32-wasip1", "roundtrip"));
    let output = Command::new(&wasmtime)
        .arg(&wasi)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", wasmtime.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !stdout.contains("roundtrip ok 333283335000") {
        return Err(format!(
            "roundtrip on wasm32-wasip1 failed: {stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    println!("wasm: wasm32-wasip1 {}", stdout.trim().replace('\n', "; "));
    let bare = shipped_path(&example(&root, "wasm32-unknown-unknown", "roundtrip"));
    invoke_main(&wasmtime, &bare, "roundtrip")?;
    // The allocator's over-aligned `realloc` (crates/mm-wasm-alloc).
    let status = crate::shipped_build("wasm32-unknown-unknown")
        .args(["--package", "mm-wasm-alloc", "--example", "realloc"])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err("shipped build of the mm-wasm-alloc realloc example failed".to_owned());
    }
    let realloc = optimize(&example(&root, "wasm32-unknown-unknown", "realloc"))?;
    invoke_main(&wasmtime, &realloc, "mm-wasm-alloc realloc")
}

/// Runs the exported `main` of a `wasm32-unknown-unknown` example under
/// wasmtime, which must return 0.
fn invoke_main(wasmtime: &Path, module: &Path, what: &str) -> Result<(), String> {
    let output = Command::new(wasmtime)
        .args(["run", "--invoke", "main"])
        .arg(module)
        .args(["0", "0"])
        .output()
        .map_err(|error| format!("cannot run {}: {error}", wasmtime.display()))?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != "0" {
        return Err(format!(
            "{what} on wasm32-unknown-unknown failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    println!("wasm: wasm32-unknown-unknown {what} ok (main returned 0)");
    Ok(())
}

/// `cargo xtask wasm-host`: the mm-wasm guest against the reference
/// host. Builds `crates/mm-wasm-guest` as shipped (no `test-hooks`) for
/// both targets and optimizes it, then runs clippy and the release tests of
/// `crates/mm-wasm-host` (its own workspace, so wasmtime stays out of the
/// root lockfile and of `cargo xtask check`) once against each module:
/// parity with the native facade on every export, images crossing
/// between native and wasm, and the upgrade policy (one runtime draining
/// an older build, here the guest built with `mod-uuid`, beside the new
/// one). Required before pushing changes to the wasm
/// crates, the facade API or the snapshot format (AGENTS.md).
pub(crate) fn host() -> Result<(), String> {
    let root = workspace_root();
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let dir = root.join("crates/mm-wasm-host");
    let mut guests = Vec::new();
    for target in TARGETS {
        let status = crate::shipped_build(target)
            .args(["--package", "mm-wasm-guest"])
            .status()
            .map_err(|error| format!("cannot run cargo: {error}"))?;
        if !status.success() {
            return Err(format!(
                "shipped build of mm-wasm-guest for {target} failed"
            ));
        }
        let module = root.join(format!("target/{target}/release/mm_wasm_guest.wasm"));
        let shipped = optimize(&module)?;
        println!(
            "wasm-host: {target} mm_wasm_guest.wasm {} bytes, {} after wasm-opt -Oz",
            file_size(&module)?,
            file_size(&shipped)?
        );
        guests.push((target, shipped));
    }
    // The bundle guest: the same tests, its host serving the bytecode from
    // the module's custom sections.
    guests.push((
        "wasm32-unknown-unknown (bundle guest)",
        shipped_bundle_guest("wasm32-unknown-unknown")?,
    ));
    // A second build for the upgrade test (tests/upgrade.rs): the same
    // source with the opt-in `uuid` module, so its build id differs. Its
    // own target directory keeps the main build cached.
    let alt = root.join("target/mm-wasm-alt");
    let status = crate::shipped_build("wasm32-unknown-unknown")
        .args(["--package", "mm-wasm-guest", "--features", "mod-uuid"])
        .env("CARGO_TARGET_DIR", &alt)
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err("shipped build of mm-wasm-guest with mod-uuid failed".to_owned());
    }
    let old = optimize(&alt.join("wasm32-unknown-unknown/release/mm_wasm_guest.wasm"))?;
    let host_cargo = |args: &[&str]| {
        let mut command = Command::new(&cargo);
        command
            .current_dir(&dir)
            .args(args)
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .env_remove("RUSTC_BOOTSTRAP")
            .env_remove("CARGO_TARGET_DIR");
        command
    };
    let status = host_cargo(&[
        "clippy",
        "--release",
        "--locked",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ])
    .status()
    .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err("clippy on crates/mm-wasm-host failed".to_owned());
    }
    for (target, guest) in guests {
        let status = host_cargo(&["test", "--release", "--locked"])
            .env("MM_WASM_GUEST", &guest)
            .env("MM_WASM_GUEST_OLD", &old)
            .status()
            .map_err(|error| format!("cannot run cargo: {error}"))?;
        if !status.success() {
            return Err(format!(
                "mm-wasm-host tests against the {target} guest failed"
            ));
        }
        println!("wasm-host: {target} guest passes the mm-wasm-host tests");
    }
    Ok(())
}

/// Builds the bundle guest (`crates/mm-wasm-guest` with `host-bundle`) as
/// shipped for `target`, in its own target directory so the default
/// guest's build stays cached, optimizes it, then appends the bytecode it
/// reads through `mm_host.bundle_read` as the custom sections `mm.prelude`
/// and `mm.stdlib` (`mm_wasm_abi::section`), from the build scripts'
/// `prelude_bundle.bin` and `stdlib_bundle.bin`. Returns
/// `shipped/mm_wasm_guest_bundle.wasm` beside the optimized module.
pub(crate) fn shipped_bundle_guest(target: &str) -> Result<PathBuf, String> {
    let root = workspace_root();
    bundle_guest_in(&root, &root.join("target/mm-wasm-bundle"), target)
}

/// [`shipped_bundle_guest`] for the tree at `src`, built into `dir`.
pub(crate) fn bundle_guest_in(src: &Path, dir: &Path, target: &str) -> Result<PathBuf, String> {
    let status = crate::shipped_build(target)
        .current_dir(src)
        .args(["--package", "mm-wasm-guest", "--features", "host-bundle"])
        .env("CARGO_TARGET_DIR", dir)
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(format!(
            "shipped build of the mm-wasm bundle guest for {target} failed"
        ));
    }
    let release = dir.join(format!("{target}/release"));
    let optimized = optimize(&release.join("mm_wasm_guest.wasm"))?;
    let mut module = fs::read(&optimized)
        .map_err(|error| format!("cannot read {}: {error}", optimized.display()))?;
    for (name, package, file) in [
        ("mm.prelude", "mm-engine", "prelude_bundle.bin"),
        ("mm.stdlib", "mm-stdlib", "stdlib_bundle.bin"),
    ] {
        let data = build_output(&release.join("build"), package, file)?;
        let mut body = Vec::new();
        leb(&mut body, name.len());
        body.extend_from_slice(name.as_bytes());
        body.extend_from_slice(&data);
        module.push(0);
        leb(&mut module, body.len());
        module.extend_from_slice(&body);
    }
    let out = optimized.with_file_name("mm_wasm_guest_bundle.wasm");
    fs::write(&out, module).map_err(|error| format!("cannot write {}: {error}", out.display()))?;
    Ok(out)
}

/// Appends `n` as LEB128.
fn leb(out: &mut Vec<u8>, mut n: usize) {
    while n >= 0x80 {
        out.push(u8::try_from(n & 0x7f).unwrap_or(0) | 0x80);
        n >>= 7;
    }
    out.push(u8::try_from(n).unwrap_or(0));
}

/// `file` in the newest `out` directory of `package`'s build script under
/// `build` (cargo's `target/<target>/release/build`).
fn build_output(build: &Path, package: &str, file: &str) -> Result<Vec<u8>, String> {
    let entries =
        fs::read_dir(build).map_err(|error| format!("cannot read {}: {error}", build.display()))?;
    let newest = entries
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_str()
                .and_then(|n| n.strip_prefix(package))
                .is_some_and(|rest| rest.starts_with('-'))
        })
        .map(|e| e.path().join("out").join(file))
        .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
        .max();
    let (_, path) = newest.ok_or_else(|| format!("no {file} from {package}'s build script"))?;
    fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

/// The last step of a shipped wasm build: `wasm-opt -Oz` (binaryen,
/// installed by `.agents/setup`; `$WASM_OPT`, `~/.local/bin`, else `PATH`)
/// writes `module` optimized for size to [`shipped_path`], which it returns.
/// The cargo output stays as it is, so a rebuild with nothing to do
/// optimizes the same input again.
pub(crate) fn optimize(module: &Path) -> Result<PathBuf, String> {
    let wasm_opt = tool("WASM_OPT", "wasm-opt").ok_or(
        "wasm-opt (binaryen) not found ($WASM_OPT, ~/.local/bin, PATH); run .agents/setup",
    )?;
    let shipped = shipped_path(module);
    if let Some(dir) = shipped.parent() {
        fs::create_dir_all(dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    }
    let output = Command::new(&wasm_opt)
        .arg("-Oz")
        .args(WASM_FEATURES)
        .arg(module)
        .arg("-o")
        .arg(&shipped)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", wasm_opt.display()))?;
    if !output.status.success() {
        return Err(format!(
            "wasm-opt -Oz {} failed: {}",
            module.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(shipped)
}

/// Where [`optimize`] writes `module`: `shipped/` beside cargo's output
/// directory, e.g. `target/wasm32-wasip1/release/shipped/hello.wasm`.
fn shipped_path(module: &Path) -> PathBuf {
    let name = module.file_name().unwrap_or_default();
    let release = module.parent().and_then(Path::parent).unwrap_or(module);
    release.join("shipped").join(name)
}

fn example(root: &Path, target: &str, name: &str) -> PathBuf {
    root.join(format!("target/{target}/release/examples/{name}.wasm"))
}

pub(crate) fn file_size(path: &Path) -> Result<u64, String> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|error| format!("cannot stat {}: {error}", path.display()))
}

/// An external tool: `$var` if set, else `~/.local/bin/<name>` (where
/// `.agents/setup` installs it), else `name` if it runs from `PATH`.
pub(crate) fn tool(var: &str, name: &str) -> Option<PathBuf> {
    if let Some(path) = env::var_os(var).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
    if let Some(home) = env::var_os("HOME") {
        let local = Path::new(&home).join(".local/bin").join(name);
        if local.is_file() {
            return Some(local);
        }
    }
    Command::new(name)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|_| PathBuf::from(name))
}

/// Builds `mm` for `wasm32-wasip1` (the same features as the native build,
/// so the same build id; in the profile `$MM_PROFILE` names, default
/// `release`) and precompiles it for wasmtime; returns wasmtime and the
/// precompiled module.
pub(crate) fn wasip1_mm() -> Result<(PathBuf, PathBuf), String> {
    let wasmtime = tool("WASMTIME", "wasmtime").ok_or(
        "mm on wasm32-wasip1 needs wasmtime ($WASMTIME, ~/.local/bin, PATH); run .agents/setup",
    )?;
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let profile = env::var("MM_PROFILE").unwrap_or_else(|_| "release".to_owned());
    let status = Command::new(cargo)
        .args([
            "build",
            "--profile",
            &profile,
            "--quiet",
            "--locked",
            "--package",
            "mm-cli",
        ])
        .args(["--target", "wasm32-wasip1"])
        .current_dir(workspace_root())
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err("release build of mm for wasm32-wasip1 failed".to_owned());
    }
    let release = workspace_root().join(format!("target/wasm32-wasip1/{profile}"));
    let module = release.join("mm.cwasm");
    let status = Command::new(&wasmtime)
        .arg("compile")
        .arg(release.join("mm.wasm"))
        .arg("-o")
        .arg(&module)
        .status()
        .map_err(|e| format!("cannot run {}: {e}", wasmtime.display()))?;
    if !status.success() {
        return Err("wasmtime compile of mm.wasm failed".to_owned());
    }
    Ok((wasmtime, module))
}
