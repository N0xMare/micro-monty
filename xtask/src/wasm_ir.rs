//! `cargo xtask wasm-ir`: wasmtime's guest instructions on the bench
//! programs against a base revision and the floors.
//!
//! Builds the working tree (or `--head`) and `--base` (default
//! `origin/main`), each other revision checked out in a worktree outside this
//! checkout (`shipped::refs_dir`, so no revision reads this checkout's
//! `.cargo/`), as shipped: `mm` for `wasm32-wasip1`, precompiled by
//! `wasmtime compile --target` for this architecture's baseline ISA, not
//! this host's CPU, so the code and the counts are the same on every
//! machine. Each program runs once per build under `valgrind
//! --tool=callgrind --dump-instr=yes` as `wasmtime run --profile=perfmap mm
//! run PROGRAM`. The count is the instructions executed at addresses in
//! wasmtime's perf map, the guest's compiled code, plus everything under
//! wasmtime's `memory.copy` and `memory.fill` libcalls, which run outside
//! it (0.7% of interp to 4.9% of str [M]): a loop turned into a bulk copy
//! moves work, not gains it. wasmtime's other runtime code, the host calls
//! and the loader are left out. The output must equal the program's line
//! in `bench/expected.txt`.
//!
//! Another revision is built with its own toolchain and, when its xtask
//! has `shipped-mm --target` ([`crate::shipped::SHIPPED_MM_TARGET`]), its
//! own recipe, else this checkout's. A committed revision's counts are
//! cached in `target/wasm-ir/cache/`, keyed by the counting method, the
//! rustc, wasmtime, `wasm-opt` and valgrind versions, `wasm-opt`'s
//! arguments and features and the shipped build's flags ([`key`]), the
//! host, and that revision's recipe and toolchain, each program's count
//! also by its source's hash (the programs are this checkout's `bench/`,
//! whatever the revision). `--package mm-cli` and the `wasmtime compile`
//! and `run` flags are not in it: change [`COUNTING`] when they change.
//!
//! Each count's floor, the lowest since the last tag, is in
//! `xtask/ir-floors/wasm-ir.txt` ([`crate::floors`]). Against the base and
//! the floor, the gate fails when a program's count rises past 1.0%
//! (`PROGRAM_LIMIT`), the geometric mean of the ratios past 0.5%
//! (`GEOMEAN_LIMIT`), or `startup`'s count, about 0.67M, by more than 300
//! instructions ([`LIMITS`]); `startup` is left out of the geomean.
//! Unrelated commits move the run loop's register allocation by up to
//! about 1% either way; the floor bounds a creep of commits each under the
//! limits. Fewer instructions always pass, with the table printed, and
//! leave the floor file alone. An intended rise takes `Wasm-IR-Accepted: <names>
//! <ceiling> <reason>` in its commit message (`MM_WASM_IR_ACCEPT` with the
//! same text before committing), which passes only the names it lists up
//! to the ceiling and raises their floors to the head's counts.
//!
//! Unlike `wasm-wall`, the count does not depend on load or on the host,
//! so one run decides. It catches what wasm-opt and Cranelift do to the
//! run loop's code (a spill, an inlining flip) at the commit that does it,
//! either way; it does not see layout costs that leave the executed
//! instructions alone.

use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, PoisonError},
    thread,
};

use crate::{
    accept::{self, Failure, Limits},
    bench::{PROGRAMS, read_expected},
    floors,
    wasm_wall::{build_mm, checkout, git, head_id},
    workspace_root,
};

/// The limits: a program +1.0%; `startup` (about 0.67M) a few hundred
/// instructions, since startup drift is not accepted (owner) and the
/// guest's count is exact (head and base of one commit +0 [M]), so an
/// intended rise takes a `startup` accept line (size review of A+B).
const LIMITS: Limits = Limits {
    program: 1.010,
    startup: 300,
};
const PROGRAM_LIMIT: f64 = LIMITS.program;
/// The most the geometric mean of the ratios (without `startup`) may rise.
const GEOMEAN_LIMIT: f64 = 1.005;
/// What a count holds, in the cache key: change it when that changes.
const COUNTING: &str = "guest code and memory.copy/fill libcalls, 2";
/// wasmtime's libcalls for `memory.copy` and `memory.fill`.
const BULK: [&str; 2] = [
    "wasmtime::runtime::vm::libcalls::raw::memory_copy",
    "wasmtime::runtime::vm::libcalls::raw::memory_fill",
];
/// The line that accepts a rise ([`crate::accept`]).
const ACCEPTED: &str = "Wasm-IR-Accepted:";
/// One program's instructions: in guest code, and under the bulk libcalls.
#[derive(Clone, Copy, Default)]
struct Count {
    guest: u64,
    bulk: u64,
}

impl Count {
    fn total(self) -> u64 {
        self.guest + self.bulk
    }
}

/// Counts per program name.
type Counts = BTreeMap<String, Count>;

/// A build to count: the working tree (`sha` none) or a commit, built in
/// `target/wasm-ir/<target>`; `label` names its files.
struct Side {
    label: String,
    target: &'static str,
    name: String,
    sha: Option<String>,
}

#[allow(
    clippy::too_many_lines,
    reason = "sides, floors, counts and the verdict in order"
)]
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut base = "origin/main".to_owned();
    let (mut head, mut only, mut write) = (None::<String>, None::<String>, false);
    let mut baselines = None::<PathBuf>;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| {
            args.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs {what}"))
        };
        match arg.as_str() {
            "--base" => base = value("a revision")?,
            "--head" => head = Some(value("a revision")?),
            "--programs" => only = Some(value("a list")?),
            "--write-floors" => write = true,
            "--baselines" => baselines = Some(PathBuf::from(value("a file")?)),
            other => return Err(format!("wasm-ir: unknown option `{other}`")),
        }
    }
    let programs = select(only.as_deref())?;
    let tools = tools()?;
    let root = workspace_root();
    let sha = |rev: &str| {
        git(
            &root,
            &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
        )
    };
    let short = |sha: &str| sha[..sha.len().min(10)].to_owned();

    let head_side = match &head {
        Some(rev) => {
            let s = sha(rev)?;
            Side {
                label: "head".to_owned(),
                target: "head",
                name: format!("{rev} ({})", short(&s)),
                sha: Some(s),
            }
        }
        None => Side {
            label: "head".to_owned(),
            target: "head",
            name: format!("working tree ({})", head_id(&root)?),
            sha: None,
        },
    };
    if let Some(file) = baselines {
        return against_baselines(&root, &tools, head_side, &programs, head.is_some(), &file);
    }

    let base_sha = git(&root, &["merge-base", &base, "HEAD"])?;
    let tag = git(
        &root,
        &[
            "describe",
            "--exact-match",
            "--tags",
            "--match",
            "v*",
            &base_sha,
        ],
    )
    .is_ok();
    if tag && write && only.is_some() {
        return Err(
            "wasm-ir: --write-floors with the base at a tag seeds the whole floor file, so it \
             counts every program: drop --programs"
                .to_owned(),
        );
    }
    let mut sides = vec![
        head_side,
        Side {
            label: "base".to_owned(),
            target: "base",
            name: format!("{base} ({})", short(&base_sha)),
            sha: Some(base_sha.clone()),
        },
    ];
    // The floors (crate::floors), and the build of each stale one.
    let bench = root.join("bench");
    let mut keys = BTreeMap::new();
    for &(name, stem) in &programs {
        let path = bench.join(format!("{stem}.py"));
        let source = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        keys.insert(
            name.to_owned(),
            floors::key(&tools.full_key(), &source_hash(&source)),
        );
    }
    let stale = floors::stale(&root, "wasm-ir", &base_sha, tag, write, &keys)?;
    for (s, _) in &stale {
        sides.push(Side {
            label: format!("floor-{}", short(s)),
            target: "floor",
            name: format!("{} (a floor's build)", short(s)),
            sha: Some(s.clone()),
        });
    }
    for side in &sides {
        println!("wasm-ir: {:<6} = {}", side.label, side.name);
    }

    let counts = measure(&root, &tools, &sides, &programs, head.is_some())?;
    let total = |c: &Counts| -> BTreeMap<String, u64> {
        c.iter().map(|(k, v)| (k.clone(), v.total())).collect()
    };
    let totals: Vec<BTreeMap<String, u64>> = counts.iter().map(total).collect();
    let recounted = floors::recounted(&stale, &totals[2..]);
    let head_build = match sides[0].sha.as_deref() {
        Some(s) => floors::build_id(&root, Some(s))?,
        None => floors::build_id(&root, None)?,
    };
    let var = "MM_WASM_IR_ACCEPT";
    let accepts = accept::for_change(&root, ACCEPTED, &base_sha, var)?;
    let raised = |name: &str, r: f64| {
        accepts
            .iter()
            .any(|a| a.names(None, name) && r <= a.ceiling())
    };
    let floor_sides = floors::Sides {
        base: &base_sha,
        tag,
        head: &totals[0],
        base_counts: &totals[1],
        keys: &keys,
        head_build: &head_build,
        recounted: &recounted,
    };
    let past = |name: &str, h: u64, o: u64| LIMITS.past(name, h, o);
    // An accepted geomean past its limit against the floors raises every
    // program that rose, so the next landing does not fail on it again.
    let (floor, _) = floors::judge(
        &root,
        "wasm-ir",
        &floor_sides,
        &floors::Rules {
            accepted: &raised,
            past: &past,
            swept: &|_| false,
            band: floors::exact,
        },
        false,
    )?;
    let geo = floors::geomean(&totals[0], &floor, &totals[1], &|name| name != "startup");
    let swept = geo > GEOMEAN_LIMIT && raised("geomean", geo);
    let (against, errors) = floors::judge(
        &root,
        "wasm-ir",
        &floor_sides,
        &floors::Rules {
            accepted: &raised,
            past: &past,
            swept: &|name| swept && name != "startup",
            band: floors::exact,
        },
        write,
    )?;
    let floor: Counts = against
        .into_iter()
        .map(|(k, n)| (k, Count { guest: n, bulk: 0 }))
        .collect();
    let failures = report(
        &programs,
        &counts[0],
        (&sides[1].name, &counts[1]),
        Some(("floor", &floor)),
    );
    floors::verdict(
        accept::verdict("wasm-ir", ACCEPTED, var, &failures, &accepts),
        &errors,
    )
}

/// `--baselines FILE`: the head against counts stored in `FILE` (such as
/// `bench/baselines-0.0.2.txt`, a release plan's speed baselines counted
/// once from revisions outside this repository): its `key` line and its
/// `wasm-ir PROGRAM GUEST BULK` lines, tab-separated. Prints the table;
/// advisory, so it fails only when the head cannot be counted.
fn against_baselines(
    root: &Path,
    tools: &Tools,
    head: Side,
    programs: &[(&str, &str)],
    head_rev: bool,
    file: &Path,
) -> Result<(), String> {
    let text = fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    let mut key = String::new();
    let mut stored = Counts::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f.as_slice() {
            ["key", "wasm-ir", k] => (*k).clone_into(&mut key),
            ["wasm-ir", name, guest, bulk] => {
                let parse = |n: &str| {
                    n.parse()
                        .map_err(|_| format!("{}: bad count in `{line}`", file.display()))
                };
                let count = Count {
                    guest: parse(guest)?,
                    bulk: parse(bulk)?,
                };
                stored.insert((*name).to_owned(), count);
            }
            _ => {}
        }
    }
    if let Some((name, _)) = programs.iter().find(|(n, _)| !stored.contains_key(*n)) {
        return Err(format!("{}: no wasm-ir count for {name}", file.display()));
    }
    println!("wasm-ir: head   = {}", head.name);
    println!("wasm-ir: base   = {} ({key})", file.display());
    if key != tools.key {
        println!("wasm-ir: the head is counted with {}", tools.key);
    }
    let counts = measure(root, tools, &[head], programs, head_rev)?;
    let past = report(
        programs,
        &counts[0],
        (&file.display().to_string(), &stored),
        None,
    );
    let past: Vec<&str> = past.iter().map(|f| f.text.as_str()).collect();
    if past.is_empty() {
        println!("\nwasm-ir: within the limits of the stored baselines");
    } else {
        println!(
            "\nwasm-ir: past the limits of the stored baselines (advisory): {}",
            past.join("; ")
        );
    }
    Ok(())
}

/// The bench programs `only` names (a comma-separated list), or all.
fn select(only: Option<&str>) -> Result<Vec<(&'static str, &'static str)>, String> {
    let Some(list) = only else {
        return Ok(PROGRAMS.to_vec());
    };
    list.split(',')
        .map(|n| {
            PROGRAMS
                .iter()
                .find(|(name, _)| *name == n)
                .copied()
                .ok_or_else(|| format!("wasm-ir: no bench program `{n}`"))
        })
        .collect()
}

/// FNV-1a of a bench program's source, which keys its cached counts.
pub(crate) fn source_hash(bytes: &[u8]) -> String {
    let h = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{h:016x}")
}

/// The tools a count runs, found, and the cache key they make.
fn tools() -> Result<Tools, String> {
    if !cfg!(target_os = "linux") {
        return Err("wasm-ir needs valgrind, which runs on Linux only".to_owned());
    }
    let valgrind = crate::wasm::tool("VALGRIND", "valgrind")
        .ok_or("wasm-ir needs valgrind ($VALGRIND, PATH); run .agents/setup")?;
    let wasmtime = crate::wasm::tool("WASMTIME", "wasmtime")
        .ok_or("wasm-ir needs wasmtime ($WASMTIME, ~/.local/bin, PATH); run .agents/setup")?;
    let wasm_opt = crate::wasm::tool("WASM_OPT", "wasm-opt")
        .ok_or("wasm-ir needs wasm-opt ($WASM_OPT, ~/.local/bin, PATH); run .agents/setup")?;
    let isa = format!("{}-unknown-linux-gnu", env::consts::ARCH);
    // Every revision builds with xtask's toolchain (rustup's
    // `RUSTUP_TOOLCHAIN`, which cargo passes on), not its own pin.
    let rustc = env::var_os("RUSTC").map_or_else(|| PathBuf::from("rustc"), PathBuf::from);
    let key = key(
        &isa,
        &version(&rustc),
        &version(&wasmtime),
        &version(&wasm_opt),
        &version(&valgrind),
    );
    let host = floors::host(&valgrind);
    Ok(Tools {
        valgrind,
        wasmtime,
        isa,
        key,
        host,
    })
}

/// The cache key: the counting method, the ISA, the tools' versions, and
/// how xtask builds a revision.
fn key(isa: &str, rustc: &str, wasmtime: &str, wasm_opt: &str, valgrind: &str) -> String {
    [
        COUNTING,
        isa,
        wasmtime,
        wasm_opt,
        &crate::wasm::wasm_opt_args("wasm32-wasip1").map_or_default(|a| a.join(" ")),
        &crate::wasm::WASM_FEATURES.join(" "),
        crate::shipped::SHIPPED_RUSTFLAGS,
        &crate::shipped::SHIPPED_CARGO.join(" "),
        rustc,
        valgrind,
    ]
    .join("; ")
}

/// The tools a count runs, and the cache key they make.
struct Tools {
    valgrind: PathBuf,
    wasmtime: PathBuf,
    isa: String,
    /// What a stored baseline's `key` line holds ([`key`]).
    key: String,
    /// [`floors::host`]: counts move with glibc and the CPU class.
    host: String,
}

impl Tools {
    /// The key a cached count and a floor are held under.
    fn full_key(&self) -> String {
        format!("{}; {}", self.key, self.host)
    }
}

/// Each side's counts: cached ones first; only what is missing is built
/// and run, then cached for a commit. `head_rev`: the head is a commit in
/// its own worktree, not the working tree.
fn measure(
    root: &Path,
    tools: &Tools,
    sides: &[Side],
    programs: &[(&str, &str)],
    head_rev: bool,
) -> Result<Vec<Counts>, String> {
    let dir = root.join("target/wasm-ir");
    let cache = dir.join("cache");
    let bench = root.join("bench");
    let expected = read_expected(&bench.join("expected.txt"))?;
    let scratch = dir.join("callgrind");
    fs::create_dir_all(&scratch).map_err(|error| format!("{}: {error}", scratch.display()))?;
    let mut hashes = BTreeMap::new();
    for &(name, stem) in programs {
        let path = bench.join(format!("{stem}.py"));
        let source = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        hashes.insert(name.to_owned(), source_hash(&source));
    }
    let mut counts: Vec<Counts> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    let mut jobs: Vec<(usize, &str, &str, PathBuf)> = Vec::new();
    for (i, side) in sides.iter().enumerate() {
        // Another revision is checked out and built outside this checkout,
        // so it reads none of this checkout's `.cargo/`, with its own
        // toolchain and, when its xtask can (`shipped-mm --target`), its
        // own recipe; the key holds both.
        let refs = crate::shipped::refs_dir("wasm-ir");
        let (tree, target) = match &side.sha {
            Some(s) if side.target != "head" || head_rev => {
                let tree = refs.join(format!("{}-src", side.target));
                checkout(root, &tree, s)?;
                (tree, refs.join(side.target))
            }
            _ => (root.to_path_buf(), dir.join(side.target)),
        };
        let own = tree == root || own_recipe(root, side.sha.as_deref());
        let recipe = crate::size::recipe_key(if own { &tree } else { root }, &tree)?;
        let key = format!("{}; {recipe}", tools.full_key());
        let mut cached = side
            .sha
            .as_deref()
            .map_or_default(|s| load(&cache, s, &key, &hashes));
        if cached.len() < programs.len()
            && let Some(path) = side
                .sha
                .as_deref()
                .and_then(|sha| crate::tag_reference::file(root, sha, "wasm-ir.tsv"))
        {
            cached.extend(read_cache(&path, &key, &hashes));
        }
        let missing: Vec<(&str, &str)> = programs
            .iter()
            .copied()
            .filter(|(name, _)| !cached.contains_key(*name))
            .collect();
        if missing.is_empty() {
            println!("wasm-ir: {} counts cached", side.label);
        } else {
            let built = if own && tree != root {
                build_itself(&tree, &tools.wasmtime, &tools.isa)?
            } else {
                build_mm(&tree, &target, &tools.wasmtime, Some(&tools.isa))?
            };
            // Sides share a target directory: each keeps its own module.
            let mm = dir.join(format!("{}.cwasm", side.label));
            fs::copy(&built, &mm).map_err(|error| format!("{}: {error}", mm.display()))?;
            jobs.extend(missing.into_iter().map(|(n, s)| (i, n, s, mm.clone())));
        }
        counts.push(cached);
        keys.push(key);
    }
    let results = parallel(jobs.len(), |j| {
        let (i, name, stem, ref mm) = jobs[j];
        count(
            &tools.valgrind,
            &tools.wasmtime,
            mm,
            &bench,
            stem,
            expected.get(name).map_or("", String::as_str),
            &scratch.join(format!("{}.{stem}", sides[i].label)),
        )
    });
    for ((i, name, _, _), result) in jobs.iter().zip(results) {
        counts[*i].insert((*name).to_owned(), result?);
    }
    for ((side, c), key) in sides.iter().zip(&counts).zip(&keys) {
        if let Some(s) = &side.sha {
            save(&cache, s, key, &hashes, c)?;
        }
    }
    Ok(counts)
}

/// Whether commit `sha`'s xtask builds its own wasip1 `mm` with its own
/// recipe (`cargo xtask shipped-mm --target`).
fn own_recipe(root: &Path, sha: Option<&str>) -> bool {
    let Some(sha) = sha else { return true };
    let file = format!("{sha}:xtask/src/shipped.rs");
    git(root, &["show", &file])
        .unwrap_or_default()
        .contains(crate::shipped::SHIPPED_MM_TARGET)
}

/// The tree at `tree` builds its shipped wasip1 `mm` with its own xtask
/// (`cargo xtask shipped-mm --target wasm32-wasip1`, which prints the
/// optimized module's path) and toolchain; precompiled for `isa`.
fn build_itself(tree: &Path, wasmtime: &Path, isa: &str) -> Result<PathBuf, String> {
    let output = crate::shipped::tree_cargo(tree)
        .args(["xtask", "shipped-mm", "--target", "wasm32-wasip1"])
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    match stdout.lines().last() {
        Some(path) if output.status.success() => {
            crate::wasm_wall::compile(Path::new(path.trim()), wasmtime, Some(isa))
        }
        _ => Err(format!(
            "`cargo xtask shipped-mm --target wasm32-wasip1` failed in {}",
            tree.display()
        )),
    }
}

/// `tool --version`'s first line, for the cache key.
pub(crate) fn version(tool: &Path) -> String {
    Command::new(tool)
        .arg("--version")
        .output()
        .map_or_default(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_owned()
        })
}

/// The cached counts of commit `sha` under `key`; none when the key
/// differs, and none for a program whose source hash is not `hashes`'.
fn load(cache: &Path, sha: &str, key: &str, hashes: &BTreeMap<String, String>) -> Counts {
    read_cache(&cache.join(format!("{sha}.tsv")), key, hashes)
}

fn read_cache(path: &Path, key: &str, hashes: &BTreeMap<String, String>) -> Counts {
    let Ok(text) = fs::read_to_string(path) else {
        return Counts::new();
    };
    let mut lines = text.lines();
    if lines.next() != Some(&format!("# {key}")) {
        return Counts::new();
    }
    lines
        .filter_map(|line| {
            let mut f = line.split('\t');
            let name = f.next()?.to_owned();
            let guest = f.next()?.parse().ok()?;
            let bulk = f.next()?.parse().ok()?;
            (hashes.get(&name).map(String::as_str) == f.next()).then_some(())?;
            Some((name, Count { guest, bulk }))
        })
        .collect()
}

fn save(
    cache: &Path,
    sha: &str,
    key: &str,
    hashes: &BTreeMap<String, String>,
    counts: &Counts,
) -> Result<(), String> {
    fs::create_dir_all(cache).map_err(|error| format!("{}: {error}", cache.display()))?;
    let mut text = format!("# {key}\n");
    for (name, c) in counts {
        let hash = hashes.get(name).map_or("", String::as_str);
        let _ = writeln!(text, "{name}\t{}\t{}\t{hash}", c.guest, c.bulk);
    }
    let path = cache.join(format!("{sha}.tsv"));
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

/// `job(0..n)` on at most one thread per CPU (a count does not depend on
/// load); the results in order.
pub(crate) fn parallel<T: Send>(n: usize, job: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let next = Mutex::new(0_usize);
    let done = Mutex::new(Vec::with_capacity(n));
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(n);
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = {
                        let mut at = next.lock().unwrap_or_else(PoisonError::into_inner);
                        *at += 1;
                        *at - 1
                    };
                    if i >= n {
                        break;
                    }
                    let result = job(i);
                    done.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((i, result));
                }
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_else(PoisonError::into_inner);
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, result)| result).collect()
}

/// Prints the table; returns the limits exceeded against base and floor.
fn report(
    programs: &[(&str, &str)],
    head: &Counts,
    base: (&str, &Counts),
    floor: Option<(&str, &Counts)>,
) -> Vec<Failure> {
    let at = |c: &Counts, name: &str| c.get(name).copied().unwrap_or_default();
    println!(
        "\nwasmtime instructions, guest code and memory.copy/fill, millions; \
         bulk = head's memory.copy/fill share"
    );
    let mut header = format!(
        "  {:<11} {:>10} {:>10} {:>9} {:>6}",
        "program", "base", "head", "head/base", "bulk"
    );
    if floor.is_some() {
        let _ = write!(header, " {:>10} {:>11}", "floor", "head/floor");
    }
    println!("{header}");
    let show = |name: &str, h: u64, o: u64| -> String {
        if name == "startup" {
            format!("{:>+9}", i128::from(h) - i128::from(o))
        } else {
            let r = ratio(h, o);
            let mark = if r > PROGRAM_LIMIT {
                "^"
            } else if r < 1.0 / PROGRAM_LIMIT {
                "v"
            } else {
                " "
            };
            format!("{r:>8.4}{mark}")
        }
    };
    for &(name, _) in programs {
        let h = at(head, name);
        let b = at(base.1, name).total();
        let mut line = format!(
            "  {name:<11} {:>10.3} {:>10.3} {} {:>5.1}%",
            millions(b),
            millions(h.total()),
            show(name, h.total(), b),
            percent(h.bulk, h.total())
        );
        if let Some((_, a)) = floor.filter(|(_, a)| a.contains_key(name)) {
            let a = at(a, name).total();
            let _ = write!(
                line,
                " {:>10.3} {:>11}",
                millions(a),
                show(name, h.total(), a)
            );
        }
        println!("{line}");
    }
    let (geo_base, mut failures) = judge(programs, head, base, floor.map(|a| a.1), None);
    let mut geo = format!("  geomean (without startup) head/base {geo_base:.4}");
    if let Some(a) = floor {
        let (geo_floor, more) = judge(programs, head, a, None, Some(base.1));
        let _ = write!(geo, ", head/floor {geo_floor:.4}");
        failures.extend(more);
    }
    println!(
        "{geo}\n  (^ past +1.0%, v below -1.0%; startup in instructions, limit +{})",
        LIMITS.startup
    );
    failures
}

/// The geomean of `head`'s ratios to `other` without `startup`, and the
/// limits it exceeds. A rise against the base that only returns to the
/// floor's level (`back`) still fails, and says so. Against the floors,
/// the geomean takes each program's base count where it is lower (`base`,
/// [`floors::lower`]).
fn judge(
    programs: &[(&str, &str)],
    head: &Counts,
    (against, other): (&str, &Counts),
    back: Option<&Counts>,
    base: Option<&Counts>,
) -> (f64, Vec<Failure>) {
    let total = |c: &Counts, name: &str| c.get(name).map_or(0, |c| c.total());
    let mut failures = Vec::new();
    let (mut product, mut n) = (1.0_f64, 0_i32);
    for &(name, _) in programs {
        // A count the other side lacks (no floor yet) is not compared.
        if !other.contains_key(name) {
            continue;
        }
        let (h, o) = (total(head, name), total(other, name));
        let failure = |text: String| Failure {
            guest: None,
            name: name.to_owned(),
            ratio: ratio(h, o),
            text,
        };
        if name == "startup" {
            if LIMITS.past(name, h, o) {
                failures.push(failure(format!("startup +{} against {against}", h - o)));
            }
            continue;
        }
        let r = ratio(h, o);
        product *= ratio(
            h,
            floors::lower(o, base.and_then(|b| b.get(name)).map(|c| c.total())),
        );
        n += 1;
        if r > PROGRAM_LIMIT {
            let note = back
                .map(|a| ratio(h, total(a, name)))
                .filter(|&ra| ra <= PROGRAM_LIMIT)
                .map_or(String::new(), |ra| {
                    format!(" (head/floor {ra:.4}: back to the floor)")
                });
            failures.push(failure(format!("{name} {r:.4} against {against}{note}")));
        }
    }
    let geomean = product.powf(1.0 / f64::from(n.max(1)));
    if geomean > GEOMEAN_LIMIT {
        failures.push(Failure {
            guest: None,
            name: "geomean".to_owned(),
            ratio: geomean,
            text: format!("geomean {geomean:.4} against {against}"),
        });
    }
    (geomean, failures)
}

#[allow(clippy::cast_precision_loss, reason = "a ratio of counts")]
fn ratio(head: u64, base: u64) -> f64 {
    head as f64 / base.max(1) as f64
}

#[allow(clippy::cast_precision_loss, reason = "display")]
fn millions(n: u64) -> f64 {
    n as f64 / 1e6
}

#[allow(clippy::cast_precision_loss, reason = "display")]
fn percent(part: u64, whole: u64) -> f64 {
    part as f64 * 100.0 / whole.max(1) as f64
}

/// One `mm run bench/STEM.py` under callgrind; its count. The callgrind
/// file and wasmtime's perf map are removed once read.
fn count(
    valgrind: &Path,
    wasmtime: &Path,
    mm: &Path,
    bench: &Path,
    stem: &str,
    want: &str,
    prefix: &Path,
) -> Result<Count, String> {
    let output = Command::new(valgrind)
        .args(["--tool=callgrind", "--dump-instr=yes"])
        .arg(format!("--callgrind-out-file={}.%p.cg", prefix.display()))
        .arg(wasmtime)
        .args(["run", "--profile=perfmap", "--allow-precompiled", "--dir"])
        .arg(format!("{}::/b", bench.display()))
        .arg(mm)
        .args(["run", &format!("/b/{stem}.py")])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("cannot run {}: {error}", valgrind.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || stdout.trim() != want {
        return Err(format!(
            "{stem}.py on {} printed {:?} (want {want:?}): {stderr}",
            mm.display(),
            stdout.trim()
        ));
    }
    // valgrind runs in the process it starts: `==PID==` heads its lines.
    let pid = stderr
        .split("==")
        .nth(1)
        .filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| format!("{stem}.py: no pid in valgrind's output"))?;
    let cg = PathBuf::from(format!("{}.{pid}.cg", prefix.display()));
    let map = PathBuf::from(format!("/tmp/perf-{pid}.map"));
    let read =
        |p: &Path| fs::read_to_string(p).map_err(|error| format!("{}: {error}", p.display()));
    let counted = match parse(&read(&cg)?, &read(&map)?) {
        Some(c) if c.guest > 0 => Ok(c),
        _ => Err(format!(
            "{stem}.py: no guest instructions in {} for the perf map {}",
            cg.display(),
            map.display()
        )),
    };
    let _ = fs::remove_file(&cg);
    let _ = fs::remove_file(&map);
    counted
}

/// From `cg` (callgrind, `--dump-instr=yes`, events `Ir`): the
/// instructions at addresses inside a function of `map` (a perf map:
/// `0xADDR SIZE NAME`, hex), and the inclusive cost of every call into a
/// `BULK` libcall. The shipped module has no name section, so its
/// functions are only indices, which move between builds: only totals
/// compare.
fn parse(cg: &str, map: &str) -> Option<Count> {
    let hex = |s: &str| u64::from_str_radix(s.strip_prefix("0x").unwrap_or(s), 16).ok();
    let mut functions: Vec<(u64, u64)> = map
        .lines()
        .filter_map(|line| {
            let mut f = line.split(' ');
            let start = hex(f.next()?)?;
            Some((start, start.checked_add(hex(f.next()?)?)?))
        })
        .collect();
    functions.sort_unstable();
    let mut out = Count::default();
    let mut last = [0_u64; 2];
    // Callgrind compresses names: `fn=(N) name` or `cfn=(N) name` defines
    // `N`, a later `cfn=(N)` refers to it.
    let mut names: BTreeMap<&str, &str> = BTreeMap::new();
    let mut bulk_callee = false;
    // The cost line after `calls=` is the call's inclusive cost.
    let mut call = false;
    for line in cg.lines() {
        if let Some(rest) = line
            .strip_prefix("fn=")
            .or_else(|| line.strip_prefix("cfn="))
        {
            let (id, name) = rest.split_once(' ').unwrap_or((rest, ""));
            if !name.is_empty() {
                names.insert(id, name);
            }
            if line.starts_with("cfn=") {
                bulk_callee = names.get(id).is_some_and(|n| BULK.contains(n));
            }
            continue;
        }
        if line.starts_with("calls=") {
            call = true;
            continue;
        }
        if !line
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'*'))
        {
            continue;
        }
        let mut f = line.split_ascii_whitespace();
        let (addr, ln, ir) = (f.next()?, f.next()?, f.next()?);
        last = [position(addr, last[0])?, position(ln, last[1])?];
        if core::mem::take(&mut call) {
            if bulk_callee {
                out.bulk += ir.parse::<u64>().ok()?;
            }
            continue;
        }
        let at = functions.partition_point(|(start, _)| *start <= last[0]);
        if at.checked_sub(1).is_some_and(|i| last[0] < functions[i].1) {
            out.guest += ir.parse::<u64>().ok()?;
        }
    }
    Some(out)
}

/// A callgrind position: absolute (`0x` hex or decimal), `+N` or `-N`
/// relative to the previous one, or `*`, the same.
fn position(s: &str, last: u64) -> Option<u64> {
    let number = |s: &str| match s.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    };
    match s.as_bytes().first()? {
        b'*' => Some(last),
        b'+' => last.checked_add(number(&s[1..])?),
        b'-' => last.checked_sub(number(&s[1..])?),
        _ => number(s),
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "tests fail on a broken fixture"
    )]

    use super::*;

    fn counts(fib: u64) -> Counts {
        Counts::from([(
            "fib".to_owned(),
            Count {
                guest: fib,
                bulk: 0,
            },
        )])
    }

    #[test]
    fn the_key_holds_how_a_revision_is_built() {
        let k = key(
            "isa",
            "rustc 1",
            "wasmtime 1",
            "wasm-opt version 133",
            "valgrind-3",
        );
        for part in crate::wasm::wasm_opt_args("wasm32-wasip1")
            .unwrap()
            .iter()
            .chain(&crate::wasm::WASM_FEATURES)
            .chain(&crate::shipped::SHIPPED_CARGO)
            .chain(&[crate::shipped::SHIPPED_RUSTFLAGS])
        {
            assert!(k.contains(part), "{part} not in {k}");
        }
        assert!(k.contains("rustc 1"), "{k}");
    }

    /// The review's G1 (`m14/X3`): dict's floor left 9% above its count by
    /// an unrecorded drop; the other 14 programs +0.47% over the base and
    /// +0.95% over their floors. Against the floors alone the geomean is
    /// 1.0030 and passes; dict's drop must not pay for the others' rises.
    #[test]
    fn a_floor_left_above_a_drop_is_no_headroom_for_the_geomean() {
        let names: Vec<String> = (0..15).map(|i| format!("p{i}")).collect();
        let programs: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), "")).collect();
        let side = |dict: u64, other: u64| -> Counts {
            names
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    let guest = if i == 0 { dict } else { other };
                    (n.clone(), Count { guest, bulk: 0 })
                })
                .collect()
        };
        let (head, base, floor) = (
            side(1_000_000, 1_009_500),
            side(1_000_000, 1_004_700),
            side(1_092_000, 1_000_000),
        );
        let geomean_failures = |base: Option<&Counts>| {
            let (geo, failures) = judge(&programs, &head, ("floor", &floor), None, base);
            let failed = failures.iter().any(|f| f.name == "geomean");
            (geo, failed)
        };
        let (geo, failed) = geomean_failures(None);
        assert!(!failed && geo < 1.0031, "the floors alone: {geo}");
        let (geo, failed) = geomean_failures(Some(&base));
        assert!(failed && geo > 1.0088, "the lower of floor and base: {geo}");
        // Against the base alone it is under its limit too: only the
        // floors see the two landings' rises together.
        let (geo, failures) = judge(&programs, &head, ("base", &base), None, None);
        assert!(geo < 1.005 && failures.is_empty(), "{geo}");
    }

    #[test]
    fn an_edited_program_is_counted_again() {
        let cache = env::temp_dir().join(format!("wasm-ir-cache-{}", std::process::id()));
        let at = |h: &str| BTreeMap::from([("fib".to_owned(), h.to_owned())]);
        save(&cache, "abc", "key", &at("1111"), &counts(100)).unwrap();
        assert_eq!(load(&cache, "abc", "key", &at("1111"))["fib"].guest, 100);
        assert!(load(&cache, "abc", "key", &at("2222")).is_empty());
        assert!(load(&cache, "abc", "other key", &at("1111")).is_empty());
        fs::remove_dir_all(&cache).unwrap();
    }
}
