//! `cargo xtask v8-ir`: V8's instructions on the bench programs, in the
//! `wasm32-unknown-unknown` mm-wasm guests, against a base revision and
//! the floors (the 0.0.2 performance review, sections 14, 15 and 17).
//!
//! Each side's two guests (`mm_wasm_guest.wasm` and
//! `mm_wasm_guest_bundle.wasm`) are got as shipped. A revision whose xtask
//! has `size --head` ([`crate::SIZE_HEAD`]) builds them itself with its own
//! recipe, in a worktree under `target/v8-ir/`, so a change to how the
//! guests are built (the `wasm-opt` flags, `optimize(size)` marks) is
//! counted at the commit that makes it; an older one is built with this
//! checkout's recipe. The head is the working tree, or with `--head-built`
//! the guests `size --head` listed in `target/size/head.tsv` (`check` runs
//! v8-ir after that stage).
//!
//! Each program runs in its own process, `node --no-liftoff
//! --single-threaded bench/count.ts guest.wasm program.py=EXPECTED` under
//! `valgrind --tool=callgrind --dump-after=uv_getrusage`: twice, each in a
//! fresh session, and `process.resourceUsage()` after the module's compile
//! and after each run ends a dump. The count is the second run's
//! instructions in code callgrind finds no object file for (`???`) and
//! that is not file-backed by the process's mappings: V8's generated code,
//! that is the guest's TurboFan code, the wasm-to-js wrappers and the
//! host's JS once V8 has compiled it. It is not the stubs in shared
//! objects (libstdc++'s allocator and locking stubs, libc) that callgrind
//! also names `???`, and, as before, not any code in a file: node's and
//! libc's `memory.copy` and `memory.fill` (11.6% of plain/str's second
//! run, 2.4% of bytearray's [M]) and interpreted JS. The rule reads the
//! process's `/proc/self/maps`, one more reason the gate is Linux only.
//! Two things make the count repeat; measured on one host, one node and
//! one valgrind [M]:
//! - `--single-threaded`: with V8's background threads the host's JS
//!   reached each tier at a point that moved with thread timing, and a
//!   count with it, between runs and machine loads, by up to 0.085%. The
//!   flag keeps V8's compiles and collections on the main thread; it also
//!   implies `--no-wasm-tier-up`, `--wasm-num-compilation-tasks=0` and
//!   `--no-wasm-async-compilation`, which change nothing under
//!   `--no-liftoff`: the guest's TurboFan code reads the same with it as
//!   without, to 15 instructions in 3.3 billion.
//! - Code in a file left out ([`generated`]): a 2-instruction libstdc++
//!   stub on the way from `operator new` to `malloc` ran 361 or 360 times,
//!   2 counts in 30 with every path, the environment, the run id and the
//!   pid width held, for no cause found; so `???` code in a file-backed
//!   mapping (libstdc++, libc, node; 860 to 1,780 instructions a program)
//!   is not counted. `count.ts` writes the mappings after the last dump;
//!   `???` code in no mapping at all fails the count rather than being
//!   guessed, and so does a dump whose costs do not add up to its
//!   `totals:` line (a write that failed).
//!
//! Each count also runs in one place ([`count`]), a fixed-length directory
//! under `/tmp` with copies of the host's JS, the guest and the program
//! under fixed names, and an empty environment, which keeps `NODE_OPTIONS`,
//! `VALGRIND_OPTS` and the like out; neither the path nor the environment
//! moved a count under the two rules above [M]. The CPU class must be
//! `x86-64-v3`, valgrind is run by its absolute path, and node's key holds
//! its binary's hash as well as its version.
//!
//! Every side runs the head's host JS. The first run also holds
//! V8's lazy compiles (read alone, they made one head 4.4% "faster" [M]),
//! and one process per program because twelve in one valgrind process
//! exited early twice [M]. Valgrind runs no AVX-512, so this is V8's AVX2
//! code. `startup` is left out: its run is a few instructions of V8 code.
//!
//! A count depends only on the guest's bytes, the program and the tools,
//! so counts are cached in `target/v8-ir/cache/` per guest content, keyed
//! by the counting method, node's version and binary, valgrind's version,
//! glibc and the CPU class ([`floors::host`]), the driver
//! (`hosts/js/bench/count.ts`, `hosts/js/src/` and `package.json`) and
//! each program's source; a commit's guests are cached per commit and recipe
//! ([`crate::size::recipe_key`]), the guests themselves in
//! `target/v8-ir/guests/`. A change to the head's recipe rebuilds the
//! guests of the commits built with it, and counts only those whose bytes
//! changed.
//!
//! Each count's floor, the lowest since the last tag, is in
//! `xtask/ir-floors/v8-ir.txt` as `<plain|bundle>/<program>`
//! ([`crate::floors`]). Against the base and the floor, a program's count
//! rising past 0.5% (`PROGRAM_LIMIT`) or the geometric mean of the ratios
//! past 0.3% (`GEOMEAN_LIMIT`), on either guest, fails unless a
//! `V8-IR-Accepted: <names> <ceiling> <reason>` line in a commit since the
//! base covers it; a name is a program or `geomean`, with `plain/` or
//! `bundle/` before it for one guest. Unrelated commits move a program by
//! 0.00-0.06% [M]. Fewer instructions always pass, and lower the floor.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread,
};

use crate::{
    accept::{self, Failure},
    bench::{PROGRAMS, read_expected},
    floors, size,
    wasm_ir::{source_hash, version},
    wasm_wall::{checkout, git, head_id},
    workspace_root,
};

/// The most one program's count may rise.
const PROGRAM_LIMIT: f64 = 1.005;
/// The most the geometric mean of a guest's ratios may rise.
const GEOMEAN_LIMIT: f64 = 1.003;
/// What a count holds, in the cache key: change it when that changes.
const COUNTING: &str = "V8 generated code (in no file), second of two runs, node \
                        --no-liftoff --single-threaded, from /tmp/mm-v8-ir-<run>/<job>, 4";
/// The line that accepts a rise ([`crate::accept`]); it raises the floors it names.
const ACCEPTED: &str = "V8-IR-Accepted:";
/// The guests counted, by their names in `size --head`'s list.
const GUESTS: [&str; 2] = ["mm_wasm_guest.wasm", "mm_wasm_guest_bundle.wasm"];

/// Counts per `guest/program`.
type Counts = BTreeMap<String, u64>;

/// A side to count: the working tree (`sha` none) or a commit.
struct Side {
    label: String,
    tree: &'static str,
    name: String,
    sha: Option<String>,
}

#[allow(
    clippy::too_many_lines,
    reason = "sides, floors, counts and the verdict in order"
)]
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut base = "origin/main".to_owned();
    let (mut only, mut head_built, mut write) = (None::<String>, false, false);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| {
            args.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs {what}"))
        };
        match arg.as_str() {
            "--base" => base = value("a revision")?,
            "--programs" => only = Some(value("a list")?),
            "--head-built" => head_built = true,
            "--write-floors" => write = true,
            other => return Err(format!("v8-ir: unknown option `{other}`")),
        }
    }
    if !cfg!(target_os = "linux") {
        return Err("v8-ir needs valgrind, which runs on Linux only".to_owned());
    }
    let valgrind = crate::wasm::tool("VALGRIND", "valgrind")
        .ok_or("v8-ir needs valgrind ($VALGRIND, PATH); run .agents/setup")?;
    // One binary for the key and the count: [`count`] clears the
    // environment, so a bare name would be looked up in another `PATH`.
    let valgrind = absolute(&valgrind)?;
    let node = crate::wasm::tool("NODE", "node").ok_or("v8-ir needs node ($NODE, PATH)")?;
    // The binary itself, which each count links into its directory ([`count`]).
    let node = Command::new(&node)
        .args(["-p", "process.execPath"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
        .ok_or_else(|| format!("cannot run {}", node.display()))?;
    let programs: Vec<(&str, &str)> = PROGRAMS
        .iter()
        .copied()
        .filter(|(name, _)| *name != "startup")
        .filter(|(name, _)| {
            only.as_deref()
                .is_none_or(|l| l.split(',').any(|n| n == *name))
        })
        .collect();
    if programs.is_empty() {
        return Err("v8-ir: no bench program selected".to_owned());
    }
    let root = workspace_root();
    let mut driver = Vec::new();
    for file in driver_files(&root)? {
        driver.extend(fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?);
    }
    let host = floors::host(&valgrind);
    // V8 picks its code by the CPU's features, which the class names only
    // in part: without AVX plain/floats counts 1.556% more [M], and
    // `x86-64-v2` covers hosts with and without it.
    if !host.ends_with(", x86-64-v3") {
        return Err(format!(
            "v8-ir counts V8's AVX2 code, so it runs only where valgrind shows the CPU as \
             x86-64-v3; here it is `{host}`, whose hosts V8 may give other code"
        ));
    }
    let (node_version, valgrind_version) = (version(&node), version(&valgrind));
    if node_version.is_empty() || valgrind_version.is_empty() {
        return Err(format!(
            "v8-ir: no version from {} or {}",
            node.display(),
            valgrind.display()
        ));
    }
    // The node binary's hash: a build at the same version (another V8
    // build, other flags) is another key.
    let node_build = fs::read(&node)
        .map(|b| source_hash(&b))
        .map_err(|e| format!("{}: {e}", node.display()))?;
    let tools = format!(
        "{COUNTING}; node {node_version} ({node_build}); {valgrind_version}; {host}; driver {}",
        source_hash(&driver)
    );
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
    // The floors (crate::floors), named `<plain|bundle>/<program>`, and
    // the build of each stale one.
    let mut keys = BTreeMap::new();
    for &(name, stem) in &programs {
        let path = root.join(format!("bench/{stem}.py"));
        let source = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for guest in GUESTS {
            let k = floors::key(&tools, &source_hash(&source));
            keys.insert(format!("{}/{name}", label(guest)), k);
        }
    }
    let stale = floors::stale(&root, "v8-ir", &base_sha, tag, &keys)?;
    let floor_commits: Vec<String> = stale.iter().map(|(s, _)| s.clone()).collect();
    let sides = sides(&root, &base, &base_sha, &floor_commits)?;
    for side in &sides {
        println!("v8-ir: {:<6} = {}", side.label, side.name);
    }
    let counts = measure(
        &root, &valgrind, &node, &tools, &sides, &programs, head_built,
    )?;
    // Counts by floor name.
    let named = |c: &Counts| -> Counts {
        c.iter()
            .filter_map(|(k, &n)| {
                let (g, p) = split(k);
                Some((format!("{}/{p}", g?), n))
            })
            .collect()
    };
    let all: Vec<Counts> = counts.iter().map(named).collect();
    let recounted = floors::recounted(&stale, &all[2..]);
    let accepts = accept::for_change(&root, ACCEPTED, &base_sha, "MM_V8_IR_ACCEPT")?;
    let raised = |name: &str, r: f64| {
        let (g, p) = name.split_once('/').unwrap_or(("", name));
        accepts
            .iter()
            .any(|a| a.names(Some(g), p) && r <= a.ceiling())
    };
    let (against, errors) = floors::judge(
        &root,
        "v8-ir",
        &floors::Sides {
            base: &base_sha,
            tag,
            head: &all[0],
            base_counts: &all[1],
            keys: &keys,
            head_build: &floors::build_id(&root, None)?,
            recounted: &recounted,
        },
        &raised,
        write,
    )?;
    // The floors keyed as the counts are.
    let floor: Counts = against
        .into_iter()
        .filter_map(|(k, n)| {
            let (g, p) = k.split_once('/')?;
            let guest = GUESTS.iter().find(|f| label(f) == g)?;
            Some((format!("{guest}/{p}"), n))
        })
        .collect();
    let mut failures = Vec::new();
    for guest in GUESTS {
        failures.extend(report(
            guest,
            &programs,
            &counts[0],
            (&sides[1].name, &counts[1]),
            Some(("floor", &floor)),
        ));
    }
    floors::verdict(
        accept::verdict("v8-ir", ACCEPTED, "MM_V8_IR_ACCEPT", &failures, &accepts),
        &errors,
    )
}

/// `tool` as an absolute path: as given when it has a directory, else the
/// first match on `PATH`.
fn absolute(tool: &Path) -> Result<PathBuf, String> {
    if tool.components().count() > 1 {
        return std::path::absolute(tool).map_err(|e| format!("{}: {e}", tool.display()));
    }
    std::env::var_os("PATH")
        .iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join(tool))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("{} is not on PATH", tool.display()))
}

/// The short name of a guest in accept lines.
fn label(guest: &str) -> &'static str {
    if guest == GUESTS[1] {
        "bundle"
    } else {
        "plain"
    }
}

/// A count's key (`<guest>/<program>`) as its guest label and program.
fn split(key: &str) -> (Option<String>, String) {
    let (guest, program) = key.split_once('/').unwrap_or(("", key));
    (Some(label(guest).to_owned()), program.to_owned())
}

/// The head (the working tree), the base (`base_sha`, the merge base with
/// `base`) and each stale floor's build (`floors`).
fn sides(root: &Path, base: &str, base_sha: &str, floors: &[String]) -> Result<Vec<Side>, String> {
    let short = |sha: &str| sha[..sha.len().min(10)].to_owned();
    let mut sides = vec![
        Side {
            label: "head".to_owned(),
            tree: "head",
            name: format!("working tree ({})", head_id(root)?),
            sha: None,
        },
        Side {
            label: "base".to_owned(),
            tree: "base",
            name: format!("{base} ({})", short(base_sha)),
            sha: Some(base_sha.to_owned()),
        },
    ];
    for c in floors {
        sides.push(Side {
            label: format!("floor-{}", short(c)),
            tree: "floor",
            name: format!("{} (a floor's build)", short(c)),
            sha: Some(c.clone()),
        });
    }
    Ok(sides)
}

/// The JS the count runs: the driver, the host it loads and the
/// `package.json` that makes them modules.
fn driver_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let js = root.join("hosts/js");
    let src = js.join("src");
    let mut files: Vec<PathBuf> = fs::read_dir(&src)
        .map_err(|e| format!("{}: {e}", src.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ts"))
        .collect();
    files.sort();
    files.push(js.join("bench/count.ts"));
    files.push(js.join("package.json"));
    Ok(files)
}

/// Each side's counts. A count depends only on the guest's bytes, the
/// program and the tools, so counts are cached per guest content, and a
/// commit's guests per commit and recipe (in `target/v8-ir/cache/`, the
/// guests themselves in `target/v8-ir/guests/`): a recipe change rebuilds
/// the guests, and only a guest whose bytes changed is counted again.
fn measure(
    root: &Path,
    valgrind: &Path,
    node: &Path,
    tools: &str,
    sides: &[Side],
    programs: &[(&str, &str)],
    head_built: bool,
) -> Result<Vec<Counts>, String> {
    let dir = root.join("target/v8-ir");
    let (cache, store) = (dir.join("cache"), dir.join("guests"));
    let bench = root.join("bench");
    let expected = read_expected(&bench.join("expected.txt"))?;
    for d in [&cache, &store] {
        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let mut hashes = BTreeMap::new();
    for &(name, stem) in programs {
        let path = bench.join(format!("{stem}.py"));
        let source = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        hashes.insert(name.to_owned(), source_hash(&source));
    }
    // Each side's guests by content, built in turn, while a pool counts
    // the programs of each new guest content as soon as it is there.
    let mut by_content: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut contents = Vec::new();
    let (tx, rx) = mpsc::channel::<(String, &str, &str)>();
    let (rx, done) = (Mutex::new(rx), Mutex::new(Vec::new()));
    let js = root.join("hosts/js");
    let driver = driver_files(root)?;
    sweep();
    make_scratch_root()?;
    let workers = thread::available_parallelism().map_or(1, usize::from);
    let built = thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let job = rx.lock().unwrap_or_else(PoisonError::into_inner).recv();
                    let Ok((id, name, stem)) = job else {
                        break;
                    };
                    let n = count(
                        valgrind,
                        node,
                        (&js, &driver),
                        &store.join(format!("{id}.wasm")),
                        &bench.join(format!("{stem}.py")),
                        expected.get(name).map_or("", String::as_str),
                    );
                    done.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((id, name, n));
                }
            });
        }
        let mut jobs = 0;
        let built = sides.iter().try_for_each(|side| {
            let ids = contents_of(root, side, head_built)?;
            for id in ids.values() {
                if by_content.contains_key(id) {
                    continue;
                }
                let cached = load(&cache, id, tools, &hashes);
                for &(name, stem) in programs {
                    if !cached.contains_key(name) {
                        jobs += 1;
                        let _ = tx.send((id.clone(), name, stem));
                    }
                }
                by_content.insert(id.clone(), cached);
            }
            contents.push(ids);
            Ok::<(), String>(())
        });
        drop(tx);
        println!(
            "v8-ir: {} guests, {jobs} programs to count",
            by_content.len()
        );
        built
    });
    let _ = fs::remove_dir_all(scratch_root());
    built?;
    for (id, name, n) in done.into_inner().unwrap_or_else(PoisonError::into_inner) {
        if let Some(c) = by_content.get_mut(&id) {
            c.insert(name.to_owned(), n?);
        }
    }
    for (id, c) in &by_content {
        save(&cache, id, tools, &hashes, c)?;
    }
    Ok(contents
        .iter()
        .map(|ids| {
            ids.iter()
                .flat_map(|(guest, id)| {
                    by_content[id]
                        .iter()
                        .map(move |(name, &n)| (format!("{guest}/{name}"), n))
                })
                .collect()
        })
        .collect())
}

/// The contents of `side`'s guests: a commit's from the cache when known
/// under its recipe, else built and stored.
fn contents_of(
    root: &Path,
    side: &Side,
    head_built: bool,
) -> Result<BTreeMap<String, String>, String> {
    let dir = root.join("target/v8-ir");
    let (cache, store) = (dir.join("cache"), dir.join("guests"));
    let (tree, own) = match &side.sha {
        Some(sha) => {
            // Outside this checkout ([`crate::shipped::refs_dir`]); checked
            // out either way, since its toolchain builds it.
            let tree = crate::shipped::refs_dir("v8-ir").join(format!("{}-src", side.tree));
            checkout(root, &tree, sha)?;
            (tree, size::builds_itself(root, sha))
        }
        None => (root.to_path_buf(), true),
    };
    let recipe = size::recipe_key(if own { &tree } else { root }, &tree)?;
    let known = side
        .sha
        .as_deref()
        .and_then(|s| load_guests(&cache, s, &recipe))
        .filter(|ids| {
            ids.values()
                .all(|id| store.join(format!("{id}.wasm")).exists())
        });
    let ids = if let Some(ids) = known {
        println!("v8-ir: the {}'s guests cached", side.label);
        ids
    } else {
        let mut ids = BTreeMap::new();
        for (guest, path) in guests(root, side, &tree, own, head_built)? {
            if !GUESTS.contains(&guest.as_str()) {
                continue;
            }
            let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let id = format!("{}-{}", source_hash(&bytes), bytes.len());
            let kept = store.join(format!("{id}.wasm"));
            fs::write(&kept, &bytes).map_err(|e| format!("{}: {e}", kept.display()))?;
            ids.insert(guest, id);
        }
        if let Some(sha) = &side.sha {
            save_guests(&cache, sha, &recipe, &ids)?;
        }
        ids
    };
    if let Some(missing) = GUESTS.iter().find(|g| !ids.contains_key(**g)) {
        return Err(format!("v8-ir: no {missing} for the {}", side.label));
    }
    Ok(ids)
}

/// A commit's guests' contents under `recipe`, when known.
fn load_guests(cache: &Path, sha: &str, recipe: &str) -> Option<BTreeMap<String, String>> {
    let text = fs::read_to_string(cache.join(format!("{sha}.guests"))).ok()?;
    let mut lines = text.lines();
    if lines.next() != Some(&format!("# {recipe}")) {
        return None;
    }
    lines
        .map(|l| {
            l.split_once('\t')
                .map(|(g, id)| (g.to_owned(), id.to_owned()))
        })
        .collect()
}

fn save_guests(
    cache: &Path,
    sha: &str,
    recipe: &str,
    ids: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut text = format!("# {recipe}\n");
    for (guest, id) in ids {
        let _ = writeln!(text, "{guest}\t{id}");
    }
    let path = cache.join(format!("{sha}.guests"));
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The paths of `side`'s guests: the head's from `size --head`'s list
/// (`head_built`) or built here, a commit's built in its worktree `tree`,
/// with its own recipe when it has one (`own`).
fn guests(
    root: &Path,
    side: &Side,
    tree: &Path,
    own: bool,
    head_built: bool,
) -> Result<BTreeMap<String, PathBuf>, String> {
    match &side.sha {
        None if head_built => paths(&root.join("target/size/head.tsv")),
        None => Ok(listed(crate::shipped_guests(root, &root.join("target"))?)),
        Some(sha) => {
            checkout(root, tree, sha)?;
            if own {
                println!("v8-ir: building the {} with its own recipe", side.label);
                paths(&size::size_head(tree)?)
            } else {
                println!(
                    "v8-ir: building the {} with the head's recipe and its own toolchain (its \
                     xtask has no `size --head`)",
                    side.label
                );
                let target = crate::shipped::refs_dir("v8-ir").join(side.tree);
                Ok(listed(crate::shipped_guests(tree, &target)?))
            }
        }
    }
}

/// The guests' paths in a list of shipped artifacts.
fn listed(artifacts: Vec<(&'static str, PathBuf)>) -> BTreeMap<String, PathBuf> {
    artifacts
        .into_iter()
        .map(|(name, path)| (name.to_owned(), path))
        .collect()
}

/// The paths `size --head` listed in `file`.
fn paths(file: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let text = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    text.lines()
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            match f[..] {
                [name, _, path] => Ok((name.to_owned(), PathBuf::from(path))),
                _ => Err(format!("{}: no path in `{l}`", file.display())),
            }
        })
        .collect()
}

/// One program's second run on `guest` under callgrind: the instructions
/// of code in no file-backed mapping ([`generated`]). Every count runs from
/// `/tmp/mm-v8-ir-<pid>/<job>/` (each part as long in every run), on copies
/// of the host's JS (`package.json`, `src/*.ts`, `bench/count.ts`), the
/// guest (`guest.wasm`) and the program (`program.py`) there, with node
/// linked in as `node`, the callgrind files there, and an empty
/// environment. That is hygiene, so that nothing of the caller's reaches
/// the process; it is not what makes the count repeat (see the module
/// doc). The directory is removed once the count is read.
fn count(
    valgrind: &Path,
    node: &Path,
    (js, driver): (&Path, &[PathBuf]),
    guest: &Path,
    program: &Path,
    want: &str,
) -> Result<u64, String> {
    let what = format!("{} on {}", program.display(), guest.display());
    let dir = scratch_dir()?;
    let counted = (|| {
        let place = |from: &Path, to: &str| {
            let to = dir.join(to);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            fs::copy(from, &to)
                .map(|_| ())
                .map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))
        };
        for file in driver {
            let local = file.strip_prefix(js).map_err(|e| e.to_string())?;
            place(file, &local.to_string_lossy())?;
        }
        place(guest, "guest.wasm")?;
        place(program, "program.py")?;
        std::os::unix::fs::symlink(node, dir.join("node"))
            .map_err(|e| format!("cannot link {} into {}: {e}", node.display(), dir.display()))?;
        let output = Command::new(valgrind)
            .args(["--tool=callgrind", "--dump-after=uv_getrusage"])
            .arg(format!("--callgrind-out-file={}/%p.cg", dir.display()))
            .arg("./node")
            .args(["--no-liftoff", "--single-threaded", "bench/count.ts"])
            .arg("guest.wasm")
            .arg(format!("program.py={want}"))
            .current_dir(&dir)
            .env_clear()
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run {}: {e}", valgrind.display()))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            let tail: Vec<&str> = stderr.lines().rev().take(6).collect();
            return Err(format!(
                "{what}: {}",
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            ));
        }
        let pid = stderr
            .split("==")
            .nth(1)
            .filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| format!("{what}: no pid in valgrind's output"))?;
        // Dump 1: start-up and the module's compile; 2: the first run; 3:
        // the second, counted.
        let read = |name: String| {
            let path = dir.join(name);
            fs::read_to_string(&path).map_err(|e| format!("{what}: {}: {e}", path.display()))
        };
        let maps = mappings(&read("maps.txt".to_owned())?);
        generated(&read(format!("{pid}.cg.3"))?, &maps).map_err(|e| format!("{what}: {e}"))
    })();
    let _ = fs::remove_dir_all(&dir);
    match counted? {
        0 => Err(format!("{what}: no instructions in generated code")),
        n => Ok(n),
    }
}

/// A new directory `/tmp/mm-v8-ir-PPPPPPPP/NNNNN`, as long for every
/// count: the run's pid in 8 digits and a 5-digit sequence number, in the
/// root [`make_scratch_root`] made. Not `TMPDIR`, so that the place is the
/// same whatever the caller's environment.
fn scratch_dir() -> Result<PathBuf, String> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed) % 100_000;
    let dir = scratch_root().join(format!("{n:05}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// Makes this run's root with mode 0700. A pid is a guessable name in a
/// shared `/tmp`, so a root that is already there is used only when this
/// user owns it (a killed run's, emptied first); one that another user
/// made, or that is not a directory, fails the run rather than letting
/// that user swap a job's files under the count.
fn make_scratch_root() -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let root = scratch_root();
    let shown = root.display();
    match fs::DirBuilder::new().mode(0o700).create(&root) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("{shown}: {e}")),
    }
    // `/proc/self` belongs to this process's user.
    let me = fs::metadata("/proc/self")
        .map_err(|e| format!("/proc/self: {e}"))?
        .uid();
    let found = fs::symlink_metadata(&root).map_err(|e| format!("{shown}: {e}"))?;
    if !found.is_dir() || found.uid() != me {
        return Err(format!(
            "{shown} is already there and is not a directory of this user's: refusing to count \
             in it"
        ));
    }
    fs::remove_dir_all(&root).map_err(|e| format!("{shown}: {e}"))?;
    // A new owner's directory in the gap fails `create` with the same
    // error as above, so the run fails rather than counting in it.
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .map_err(|e| format!("{shown}: {e}"))
}

/// This run's `/tmp/mm-v8-ir-PPPPPPPP` (its pid in 8 digits), removed when
/// the counts are in.
fn scratch_root() -> PathBuf {
    PathBuf::from(format!("/tmp/mm-v8-ir-{:08}", std::process::id()))
}

/// Removes the `/tmp/mm-v8-ir-PPPPPPPP` of runs that were killed: as
/// `durability`'s sweep, a directory whose pid is gone, and only once it is
/// a day old, which a live run's never is (each count makes a directory in
/// it). Other names, such as another tool's, stay.
fn sweep() {
    let Ok(entries) = fs::read_dir("/tmp") else {
        return;
    };
    let day = std::time::Duration::from_hours(24);
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|n| n.strip_prefix("mm-v8-ir-"))
            .filter(|p| p.len() == 8 && p.bytes().all(|b| b.is_ascii_digit()))
        else {
            continue;
        };
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|age| age > day));
        if old && !Path::new(&format!("/proc/{}", pid.trim_start_matches('0'))).exists() {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// The mappings of `maps` (a `/proc/PID/maps`): each range, and whether
/// it maps a file.
fn mappings(maps: &str) -> Vec<(u64, u64, bool)> {
    maps.lines()
        .filter_map(|line| {
            let mut f = line.split_ascii_whitespace();
            let (lo, hi) = f.next()?.split_once('-')?;
            let file = f.nth(4).is_some_and(|path| path.starts_with('/'));
            Some((
                u64::from_str_radix(lo, 16).ok()?,
                u64::from_str_radix(hi, 16).ok()?,
                file,
            ))
        })
        .collect()
}

/// The self instructions in `cg` (one callgrind dump) at code with no
/// object file, which callgrind names `???`, in the anonymous mappings of
/// `maps` ([`mappings`]): V8's generated code. `???` code in a file's
/// mapping is left out: stubs in libstdc++, libc and node that callgrind
/// does not attribute, whose calls moved for no cause found (libstdc++'s
/// `operator new` to `malloc` stub ran 361 or 360 times [M]). A `???`
/// function is named by its address; one in no mapping at all (code
/// unmapped before the maps were read) fails the count. Names are
/// compressed: `ob=(N) name` and `fn=(N) name` define `N` (`cob=` and
/// `cfn=` too), a later `fn=(N)` refers to it. The cost line after a
/// `calls=` line is the call's inclusive cost, not self cost. Callgrind
/// does not report a write that failed (a full `/tmp`), and a dump cut
/// short read lower [M], so the self costs of every object must add up
/// to the dump's `totals:` line, and a cost that is not a number fails.
fn generated(cg: &str, maps: &[(u64, u64, bool)]) -> Result<u64, String> {
    let (mut unknown, mut ob, mut call, mut n) = (Vec::new(), "", false, 0_u64);
    let (mut all, mut totals) = (0_u64, None);
    let mut names = BTreeMap::new();
    // Where the current function is: in a file, in no mapping, or else
    // generated code (`None`).
    let mut fun: (&str, Option<bool>) = ("", None);
    for line in cg.lines() {
        if let Some(rest) = line
            .strip_prefix("ob=")
            .or_else(|| line.strip_prefix("cob="))
        {
            let (id, name) = rest.split_once(' ').unwrap_or((rest, ""));
            if name == "???" {
                unknown.push(id);
            }
            if line.starts_with("ob=") {
                ob = id;
            }
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("fn=")
            .or_else(|| line.strip_prefix("cfn="))
        {
            let (id, name) = rest.split_once(' ').unwrap_or((rest, ""));
            if !name.is_empty() {
                names.insert(id, name);
            }
            if line.starts_with("fn=") {
                let name = names.get(id).copied().unwrap_or(id);
                let at = name
                    .strip_prefix("0x")
                    .and_then(|hex| u64::from_str_radix(hex.split('\'').next()?, 16).ok());
                let mapped = at.and_then(|a| {
                    maps.iter()
                        .find(|&&(lo, hi, _)| (lo..hi).contains(&a))
                        .map(|&(_, _, file)| file)
                });
                fun = (
                    name,
                    mapped.map_or(Some(false), |file| file.then_some(true)),
                );
            }
            continue;
        }
        if line.starts_with("calls=") {
            call = true;
            continue;
        }
        if let Some(t) = line.strip_prefix("totals:") {
            totals = Some(
                t.trim()
                    .parse::<u64>()
                    .map_err(|_| format!("the dump's `{line}` is not a count"))?,
            );
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
        let (Some(_), Some(ir)) = (f.next(), f.next()) else {
            continue;
        };
        if core::mem::take(&mut call) {
            continue;
        }
        let ir: u64 = ir
            .parse()
            .map_err(|_| format!("the dump's cost line `{line}` is not a count"))?;
        all += ir;
        if !unknown.contains(&ob) {
            continue;
        }
        match fun.1 {
            None => n += ir,
            Some(true) => {}
            Some(false) => {
                return Err(format!(
                    "`???` code at {} is in no mapping of the process",
                    fun.0
                ));
            }
        }
    }
    match totals {
        Some(t) if t == all => Ok(n),
        Some(t) => Err(format!(
            "the dump's costs add up to {all}, not its `totals: {t}`: cut short?"
        )),
        None => Err("the dump has no `totals:` line: cut short?".to_owned()),
    }
}

/// The cached counts of the guest with content `id` under `key`, each kept
/// only while its program's source hash is `hashes`'.
fn load(cache: &Path, id: &str, key: &str, hashes: &BTreeMap<String, String>) -> Counts {
    let Ok(text) = fs::read_to_string(cache.join(format!("{id}.tsv"))) else {
        return Counts::new();
    };
    let mut lines = text.lines();
    if lines.next() != Some(&format!("# {key}")) {
        return Counts::new();
    }
    lines
        .filter_map(|line| {
            let mut f = line.split('\t');
            let id = f.next()?.to_owned();
            let n = f.next()?.parse().ok()?;
            (hashes.get(&id).map(String::as_str) == f.next()).then_some((id, n))
        })
        .collect()
}

/// Writes `counts` for the guest with content `id`, keeping the cached
/// counts of programs not counted this time (`--programs`).
fn save(
    cache: &Path,
    id: &str,
    key: &str,
    hashes: &BTreeMap<String, String>,
    counts: &Counts,
) -> Result<(), String> {
    fs::create_dir_all(cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    let path = cache.join(format!("{id}.tsv"));
    let mut text = format!("# {key}\n");
    let old = fs::read_to_string(&path).unwrap_or_default();
    let mut lines = old.lines();
    if lines.next() == Some(&format!("# {key}")) {
        for line in lines {
            let name = line.split('\t').next().unwrap_or_default();
            if !counts.contains_key(name) {
                let _ = writeln!(text, "{line}");
            }
        }
    }
    for (name, n) in counts {
        let hash = hashes.get(name).map_or("", String::as_str);
        let _ = writeln!(text, "{name}\t{n}\t{hash}");
    }
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Prints `guest`'s table; returns the limits exceeded against base and
/// floor.
fn report(
    guest: &str,
    programs: &[(&str, &str)],
    head: &Counts,
    base: (&str, &Counts),
    floor: Option<(&str, &Counts)>,
) -> Vec<Failure> {
    let at = |c: &Counts, name: &str| c.get(&format!("{guest}/{name}")).copied().unwrap_or(0);
    println!("\nV8 instructions in generated code, {guest}, second run, millions");
    let mut header = format!(
        "  {:<11} {:>10} {:>10} {:>10}",
        "program", "base", "head", "head/base"
    );
    if floor.is_some() {
        let _ = write!(header, " {:>10} {:>11}", "floor", "head/floor");
    }
    println!("{header}");
    let show = |r: f64| {
        let mark = if r > PROGRAM_LIMIT {
            "^"
        } else if r < 1.0 / PROGRAM_LIMIT {
            "v"
        } else {
            " "
        };
        format!("{r:>9.4}{mark}")
    };
    for &(name, _) in programs {
        let (h, b) = (at(head, name), at(base.1, name));
        let mut line = format!(
            "  {name:<11} {:>10.3} {:>10.3} {}",
            millions(b),
            millions(h),
            show(ratio(h, b))
        );
        if let Some((_, a)) = floor.filter(|(_, a)| a.contains_key(&format!("{guest}/{name}"))) {
            let a = at(a, name);
            let _ = write!(line, " {:>10.3} {:>11}", millions(a), show(ratio(h, a)));
        }
        println!("{line}");
    }
    let mut failures = Vec::new();
    let mut geo = String::from("  geomean");
    for (against, other) in [Some(base), floor].into_iter().flatten() {
        let (mut product, mut n) = (1.0_f64, 0_i32);
        for &(name, _) in programs {
            // A count the other side lacks (no floor yet) is not compared.
            if !other.contains_key(&format!("{guest}/{name}")) {
                continue;
            }
            let r = ratio(at(head, name), at(other, name));
            product *= r;
            n += 1;
            if r > PROGRAM_LIMIT {
                failures.push(Failure {
                    guest: Some(label(guest).to_owned()),
                    name: name.to_owned(),
                    ratio: r,
                    text: format!("{guest} {name} {r:.4} against {against}"),
                });
            }
        }
        let geomean = product.powf(1.0 / f64::from(n.max(1)));
        let _ = write!(geo, " {geomean:.4} against {against};");
        if geomean > GEOMEAN_LIMIT {
            failures.push(Failure {
                guest: Some(label(guest).to_owned()),
                name: "geomean".to_owned(),
                ratio: geomean,
                text: format!("{guest} geomean {geomean:.4} against {against}"),
            });
        }
    }
    println!("{geo}\n  (^ past +0.5%, v below -0.5%; the geomean's limit is +0.3%)");
    failures
}

#[allow(clippy::cast_precision_loss, reason = "a ratio of counts")]
fn ratio(head: u64, base: u64) -> f64 {
    head as f64 / base.max(1) as f64
}

#[allow(clippy::cast_precision_loss, reason = "display")]
fn millions(n: u64) -> f64 {
    n as f64 / 1e6
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests fail on a broken fixture")]
    use super::{generated, mappings};

    #[test]
    fn counts_self_cost_in_generated_code() {
        let maps = "\\
0045b000-02a06000 r-xp 0005b000 103:02 1 /nix/store/x-nodejs/bin/node
04288000-04a87000 rwxp 00000000 00:00 0
07000000-07200000 r-xp 00000000 103:02 2 /nix/store/x-gcc-lib/lib/libstdc++.so.6
";
        let cg = "\\
ob=(1) /usr/bin/node
fn=(1) main
0x10 5
ob=(2) ???
fn=(2) 0x0000000004300000
0x20 7
+1 3
cob=(2)
cfn=(3) 0x00000000070ed3d0
calls=1 0x30
0x21 1000
ob=(1)
fn=(1)
0x11 11
ob=(2)
fn=(2)
* 2
fn=(3)
0x30 2
fn=(4) 0x0000000004300040'2
0x40 4

totals: 34
";
        // 7 + 3 + 2 + 4 in `???` in no file; the call's 1000 is inclusive
        // cost, node's own 5 and 11 are not generated code, and the stub's
        // 2 (named first by a `cfn=` line) are in libstdc++'s mapping.
        let maps = mappings(maps);
        assert_eq!(maps[1], (0x428_8000, 0x4a8_7000, false));
        assert_eq!(generated(cg, &maps), Ok(16));
        // Code in no mapping fails the count.
        let err = generated(cg, &maps[..1]).unwrap_err();
        assert!(err.contains("0x0000000004300000"), "{err}");
    }

    #[test]
    fn a_dump_cut_short_or_with_a_bad_cost_fails() {
        let maps = mappings("04288000-04a87000 rwxp 00000000 00:00 0\n");
        let cg = "\
ob=(2) ???
fn=(2) 0x0000000004300000
0x20 7
+1 3

totals: 10
";
        assert_eq!(generated(cg, &maps), Ok(10));
        // Cut after a cost line, or within one, as a failed write leaves it.
        let cut = &cg[..cg.find("+1 3").unwrap() + 3];
        assert!(generated(cut, &maps).unwrap_err().contains("no `totals:`"));
        let cut = &cg[..cg.find("totals").unwrap()];
        assert!(generated(cut, &maps).is_err());
        // A total that the costs do not reach, and a cost that is not a number.
        let short = cg.replace("totals: 10", "totals: 13");
        assert!(
            generated(&short, &maps)
                .unwrap_err()
                .contains("add up to 10")
        );
        let bad = cg.replace("+1 3", "+1 3x");
        assert!(generated(&bad, &maps).unwrap_err().contains("`+1 3x`"));
    }
}
