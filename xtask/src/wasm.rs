//! `cargo xtask wasm`: the facade on wasm (docs/spec.md §10.3).
//!
//! Builds `micro-monty` and its examples as shipped
//! ([`crate::shipped_build`]) for `wasm32-wasip1` and
//! `wasm32-unknown-unknown`, runs binaryen's `wasm-opt` with the shipped
//! arguments ([`wasm_opt_args`]) on the `hello` and `roundtrip` examples
//! ([`optimize`]), reports `hello`'s size on each
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

/// The features rustc 1.100 enables by default on both wasm targets
/// (`rustc --print cfg --target wasm32-unknown-unknown`, plus the two
/// LLVM implies). The stripped module carries no `target_features`
/// section, so wasm-opt is told; it must not use any other feature.
pub(crate) const WASM_FEATURES: [&str; 8] = [
    "--enable-bulk-memory",
    "--enable-bulk-memory-opt",
    "--enable-call-indirect-overlong",
    "--enable-multivalue",
    "--enable-mutable-globals",
    "--enable-nontrapping-float-to-int",
    "--enable-reference-types",
    "--enable-sign-ext",
];

/// `wasm-opt`'s arguments for a shipped module of `target`, besides
/// [`WASM_FEATURES`]: the one place they are chosen. Everything that ships
/// or measures wasm runs them through [`wasm_opt`]: `size`, `wasm`,
/// `wasm-host`, `wasm-ir`, `wasm-wall`, the release workflow (through
/// `size` and `wasm`) and, through `cargo xtask wasm-opt TARGET ...`,
/// hosts/js/scripts/build-guests.py and xtask/size_map.py.
///
/// `wasm32-wasip1` (`mm`, `hello`, the wasip1 guests: modules run by
/// wasmtime) gets `-Oz` with nothing inlined into a function past 16 KiB.
/// At binaryen's default (400 KiB) `-Oz` inlines into the run loop
/// (`Vm::dispatch`, about 36 KB of wasm before `wasm-opt`) whatever is
/// small or called once, until Cranelift spills its registers, and
/// unrelated commits flip what it inlines, moving the guest's instruction
/// counts by about 1% either way. Under the cap nothing is inlined into the
/// loop: wasmtime runs fib in 4.9% and closure in 7.5% fewer guest
/// instructions (bench geomean 0.975) [M, binaryen 133, rustc 1.100].
/// Every cap from 256 B to 32 KiB gives the same run loop; 40 KiB is the
/// edge and 48 KiB is back at the default's loop. 16 KiB is the middle of
/// that plateau. It holds only while the loop is bigger than the cap: a
/// new binaryen or a run loop shrunk below 16 KiB can move it. [`BINARYEN`]
/// stops the build on another binaryen until the plateau is measured again
/// (each cap in place of 16384 here, then `cargo xtask wasm-ir --programs
/// fib,loop,closure,classes`, whose cache key holds these arguments).
///
/// `wasm32-unknown-unknown` (the mm-wasm guests and examples V8 and
/// Workers run) repeats `-Oz` until the size converges. It keeps no inline
/// cap: counted in V8's TurboFan code the cap makes all 12 bench programs
/// worse, geomean 1.0154 (loop 1.045, generators 1.040) [M, performance review].
pub(crate) fn wasm_opt_args(target: &str) -> Result<&'static [&'static str], String> {
    match target {
        "wasm32-wasip1" => Ok(&["-Oz", "--inline-max-combined-binary-size=16384"]),
        "wasm32-unknown-unknown" => Ok(&["-Oz", "--converge"]),
        other => Err(format!(
            "no shipped wasm-opt arguments for target `{other}`"
        )),
    }
}

/// The binaryen [`wasm_opt_args`]' cap was measured on (`wasm-opt
/// --version`'s start; `.agents/setup` and the release workflow install it).
const BINARYEN: &str = "wasm-opt version 133 ";

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
        optimize(target, &example(&root, target, "roundtrip"))?;
        // `hello` as `cargo xtask size` builds it (its own crate at opt 3).
        let shipped = crate::shipped_example(target, "hello")?;
        let hello = example(&root, target, "hello");
        println!(
            "wasm: {target} hello.wasm {} bytes, {} after wasm-opt",
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
    let realloc = optimize(
        "wasm32-unknown-unknown",
        &example(&root, "wasm32-unknown-unknown", "realloc"),
    )?;
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
pub(crate) fn host(size_head_built: bool) -> Result<(), String> {
    let root = workspace_root();
    let mut guests = Vec::new();
    let mut sizes = if size_head_built {
        crate::size::paths(&root.join("target/size/head.tsv"))?
    } else {
        std::collections::BTreeMap::new()
    };
    for target in TARGETS {
        if size_head_built && target == "wasm32-unknown-unknown" {
            guests.push((
                target,
                sizes
                    .remove("mm_wasm_guest.wasm")
                    .ok_or("no default guest from size-head")?,
            ));
            continue;
        }
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
        let shipped = optimize(target, &module)?;
        println!(
            "wasm-host: {target} mm_wasm_guest.wasm {} bytes, {} after wasm-opt",
            file_size(&module)?,
            file_size(&shipped)?
        );
        guests.push((target, shipped));
    }
    // The bundle guest: the same tests, its host serving the bytecode from
    // the module's custom sections.
    guests.push((
        "wasm32-unknown-unknown (bundle guest)",
        if size_head_built {
            sizes
                .remove("mm_wasm_guest_bundle.wasm")
                .ok_or("no bundle guest from size-head")?
        } else {
            shipped_bundle_guest("wasm32-unknown-unknown")?
        },
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
    let old = optimize(
        "wasm32-unknown-unknown",
        &alt.join("wasm32-unknown-unknown/release/mm_wasm_guest.wasm"),
    )?;
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

pub(crate) fn host_cargo(args: &[&str]) -> Command {
    let mut command = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()));
    command
        .current_dir(workspace_root().join("crates/mm-wasm-host"))
        .args(args)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTC_BOOTSTRAP")
        .env_remove("CARGO_TARGET_DIR");
    command
}

/// `cargo xtask js-host`: `hosts/js`' tests (`node --test`, no `npm ci`:
/// they import only node's own modules) on the guests the gate already
/// built, once on the default guest and once with `MM_GUEST=bundle`. It
/// builds nothing: the default and bundle guests are `cargo xtask size
/// --head`'s, and the second build one test loads beside them, which only
/// needs another build id, is `cargo xtask wasm-host`'s guest with
/// `mod-uuid` (`MM_GUEST_OLD`; `npm run guests` builds `core35` for it,
/// most of a 5-minute build). `check` runs it after both stages.
pub(crate) fn js_host() -> Result<(), String> {
    const UU: &str = "wasm32-unknown-unknown";
    let root = workspace_root();
    let node = tool("NODE", "node").ok_or("js-host needs node ($NODE, PATH)")?;
    let version = Command::new(&node)
        .arg("--version")
        .output()
        .map_or_default(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned());
    // Node runs the TypeScript sources as they are from 23.6
    // (hosts/js/package.json's `engines`).
    let mut parts = version
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, minor) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    if (major, minor) < (23, 6) {
        return Err(format!(
            "js-host needs node 23.6 or later; {} is `{version}`",
            node.display()
        ));
    }
    let guests = [
        (
            "default",
            root.join(format!("target/{UU}/shipped/mm_wasm_guest.wasm")),
            "cargo xtask size --head",
        ),
        (
            "bundle",
            root.join(format!(
                "target/mm-wasm-bundle/{UU}/shipped/mm_wasm_guest_bundle.wasm"
            )),
            "cargo xtask size --head",
        ),
        (
            "old",
            root.join(format!(
                "target/mm-wasm-alt/{UU}/shipped/mm_wasm_guest.wasm"
            )),
            "cargo xtask wasm-host",
        ),
    ];
    // The tests read `<name>.wasm` in one directory.
    let dir = root.join("target/js-host/guests");
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    for (name, path, built_by) in &guests {
        let to = dir.join(format!("{name}.wasm"));
        fs::copy(path, &to).map_err(|e| {
            format!(
                "js-host: cannot copy {} ({e}); `{built_by}` builds it",
                path.display()
            )
        })?;
    }
    for guest in ["default", "bundle"] {
        let status = Command::new(&node)
            .args(["--test", "--expose-gc", "test/*.test.ts"])
            .current_dir(root.join("hosts/js"))
            .env("MM_GUEST_DIR", &dir)
            .env("MM_GUEST", guest)
            .env("MM_GUEST_OLD", "old")
            .status()
            .map_err(|e| format!("cannot run {}: {e}", node.display()))?;
        if !status.success() {
            return Err(format!("hosts/js' tests on the {guest} guest failed"));
        }
        println!("js-host: the {guest} guest passes hosts/js' tests");
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
    let status = crate::shipped::shipped_build_in(target, src)
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
    let optimized = optimize(target, &release.join("mm_wasm_guest.wasm"))?;
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
/// `build` (cargo's `target/<target>/release/build`), laid out as cargo
/// 1.100 does: `build/<package>/<hash>/out`. Directories of the older
/// `build/<package>-<hash>/out` layout, left by earlier toolchains, are
/// stale and ignored.
fn build_output(build: &Path, package: &str, file: &str) -> Result<Vec<u8>, String> {
    // Cargo 1.100 writes `build/<package>/<hash>/out`; 1.99 (the toolchain
    // v0.0.1 pins, which the size gate's release reference builds with)
    // wrote `build/<package>-<hash>/out`.
    let mut outs = Vec::new();
    if let Ok(entries) = fs::read_dir(build.join(package)) {
        outs.extend(entries.filter_map(Result::ok).map(|e| e.path().join("out")));
    }
    let prefix = format!("{package}-");
    if let Ok(entries) = fs::read_dir(build) {
        outs.extend(
            entries
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
                .map(|e| e.path().join("out")),
        );
    }
    let newest = outs
        .into_iter()
        .map(|out| out.join(file))
        .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
        .max();
    let (_, path) = newest.ok_or_else(|| {
        format!(
            "no {file} from {package}'s build script under {}",
            build.display()
        )
    })?;
    fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

/// The last step of a shipped wasm build: [`wasm_opt`] writes `module`,
/// built for `target`, optimized to [`shipped_path`], which it returns. The
/// cargo output stays as it is, so a rebuild with nothing to do optimizes
/// the same input again.
pub(crate) fn optimize(target: &str, module: &Path) -> Result<PathBuf, String> {
    let shipped = shipped_path(module);
    if let Some(dir) = shipped.parent() {
        fs::create_dir_all(dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    }
    wasm_opt(target, module, &shipped, &[])?;
    Ok(shipped)
}

/// `wasm-opt` (binaryen [`BINARYEN`], installed by `.agents/setup`;
/// `$WASM_OPT`, `~/.local/bin`, else `PATH`) with `target`'s shipped
/// arguments ([`wasm_opt_args`], [`WASM_FEATURES`]) and `extra` (such as
/// `-g`), from `input` to `output`.
pub(crate) fn wasm_opt(
    target: &str,
    input: &Path,
    output: &Path,
    extra: &[String],
) -> Result<(), String> {
    let args = wasm_opt_args(target)?;
    let wasm_opt = tool("WASM_OPT", "wasm-opt").ok_or(
        "wasm-opt (binaryen) not found ($WASM_OPT, ~/.local/bin, PATH); run .agents/setup",
    )?;
    let version = Command::new(&wasm_opt)
        .arg("--version")
        .output()
        .map_or_default(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    if !version.starts_with(BINARYEN) {
        return Err(format!(
            "{} is `{}`, not binaryen 133, which the shipped inline cap was \
             measured on: measure the plateau again on it before changing \
             BINARYEN (xtask/src/wasm.rs, wasm_opt_args), or run .agents/setup",
            wasm_opt.display(),
            version.lines().next().unwrap_or("").trim()
        ));
    }
    let run = Command::new(&wasm_opt)
        .args(args)
        .args(WASM_FEATURES)
        .args(extra)
        .arg(input)
        .arg("-o")
        .arg(output)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", wasm_opt.display()))?;
    if !run.status.success() {
        return Err(format!(
            "wasm-opt {} {} failed: {}",
            args.join(" "),
            input.display(),
            String::from_utf8_lossy(&run.stderr).trim()
        ));
    }
    Ok(())
}

/// `cargo xtask wasm-opt TARGET INPUT OUTPUT [ARG...]`: [`wasm_opt`] for
/// the scripts that build wasm outside xtask.
pub(crate) fn wasm_opt_command(args: &[String]) -> Result<(), String> {
    let [target, input, output, extra @ ..] = args else {
        return Err("usage: cargo xtask wasm-opt TARGET INPUT OUTPUT [ARG...]".to_owned());
    };
    wasm_opt(target, Path::new(input), Path::new(output), extra)
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
pub(crate) fn wasip1_mm(built: bool) -> Result<(PathBuf, PathBuf), String> {
    let wasmtime = tool("WASMTIME", "wasmtime").ok_or(
        "mm on wasm32-wasip1 needs wasmtime ($WASMTIME, ~/.local/bin, PATH); run .agents/setup",
    )?;
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let profile = env::var("MM_PROFILE").unwrap_or_else(|_| "release".to_owned());
    let release = workspace_root().join(format!("target/wasm32-wasip1/{profile}"));
    let module = release.join("mm.cwasm");
    // Only check's release graph requests this: conformance built and ran
    // the regular CLI with this wasmtime, profile and host in this run.
    if built {
        return Ok((wasmtime, module));
    }
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
