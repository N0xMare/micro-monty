//! `cargo xtask v8-ir`: V8's instructions on the bench programs, in the
//! `wasm32-unknown-unknown` mm-wasm guests, against a base revision and an
//! anchor (the 0.0.2 performance review, sections 14 and 15).
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
//! hosts/js/bench/count.ts GUEST PROGRAM=EXPECTED` under `valgrind
//! --tool=callgrind --dump-after=uv_getrusage`: twice, each in a fresh
//! session, and `process.resourceUsage()` after the module's compile and
//! after each run ends a dump. The count is the second run's instructions
//! in code callgrind finds no object file for (`???`): V8's generated
//! code, TurboFan's wasm code and a little JS. The first run also holds
//! V8's lazy compiles (read alone, they made one head 4.4% "faster" [M]),
//! and one process per program because twelve in one valgrind process
//! exited early twice [M]. Valgrind runs no AVX-512, so this is V8's AVX2
//! code. `startup` is left out: its run is a few instructions of V8 code.
//!
//! A count depends only on the guest's bytes, the program and the tools,
//! so counts are cached in `target/v8-ir/cache/` per guest content, keyed
//! by the counting method, node's and valgrind's versions, the driver
//! (`hosts/js/bench/count.ts` and `hosts/js/src/`) and each program's
//! source; a commit's guests are cached per commit and recipe
//! ([`crate::size::recipe_key`]), the guests themselves in
//! `target/v8-ir/guests/`. A change to the head's recipe rebuilds the
//! guests of the commits built with it, and counts only those whose bytes
//! changed.
//!
//! The anchor and the accept line work as `wasm-ir`'s: the latest `v*` tag
//! the base descends from, or the latest commit since it with a
//! `V8-IR-Accepted:` line; its count of a program is the least of the
//! anchor's, the base's and every first-parent commit's between them that
//! changed the build. Against both, a program's count rising past 0.5%
//! (`PROGRAM_LIMIT`) or the geometric mean of the ratios past 0.3%
//! (`GEOMEAN_LIMIT`), on either guest, fails. Unrelated commits move a
//! program by 0.00-0.06% [M]. Fewer instructions always pass.
//! `MM_V8_IR_ACCEPT=<reason>` passes an intended rise, whose commit
//! message records the table on a `V8-IR-Accepted:` line.

use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, PoisonError, mpsc},
    thread,
};

use crate::{
    bench::{PROGRAMS, read_expected},
    size,
    wasm_ir::{changed_since, default_anchor, source_hash, version},
    wasm_wall::{checkout, git, head_id},
    workspace_root,
};

/// The most one program's count may rise.
const PROGRAM_LIMIT: f64 = 1.005;
/// The most the geometric mean of a guest's ratios may rise.
const GEOMEAN_LIMIT: f64 = 1.003;
/// What a count holds, in the cache key: change it when that changes.
const COUNTING: &str = "V8 generated code, second of two runs, node --no-liftoff, 1";
/// The line that marks a commit whose rise was accepted; it re-anchors.
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

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut base = "origin/main".to_owned();
    let (mut anchor, mut only, mut head_built) = (None::<String>, None::<String>, false);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |what: &str| {
            args.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs {what}"))
        };
        match arg.as_str() {
            "--base" => base = value("a revision")?,
            "--anchor" => anchor = Some(value("a revision")?),
            "--programs" => only = Some(value("a list")?),
            "--head-built" => head_built = true,
            other => return Err(format!("v8-ir: unknown option `{other}`")),
        }
    }
    if !cfg!(target_os = "linux") {
        return Err("v8-ir needs valgrind, which runs on Linux only".to_owned());
    }
    let valgrind = crate::wasm::tool("VALGRIND", "valgrind")
        .ok_or("v8-ir needs valgrind ($VALGRIND, PATH); run .agents/setup")?;
    let node = crate::wasm::tool("NODE", "node").ok_or("v8-ir needs node ($NODE, PATH)")?;
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
    let tools = format!(
        "{COUNTING}; node {}; {}; driver {}",
        version(&node),
        version(&valgrind),
        source_hash(&driver)
    );
    let sides = sides(&root, &base, anchor.as_deref())?;
    for side in &sides {
        println!("v8-ir: {:<6} = {}", side.label, side.name);
    }
    let counts = measure(
        &root, &valgrind, &node, &tools, &sides, &programs, head_built,
    )?;
    // The anchor's counts: the least from the anchor to the base.
    let least = (sides.len() > 2).then(|| least(&counts[1..]));
    let anchor_name = sides
        .get(2)
        .map(|side| format!("{} to the base", side.name));
    let mut failures = Vec::new();
    for guest in GUESTS {
        failures.extend(report(
            guest,
            &programs,
            &counts[0],
            (&sides[1].name, &counts[1]),
            anchor_name.as_deref().zip(least.as_ref()),
        ));
    }
    verdict(&failures)
}

/// The head (the working tree), the base (the merge base with `base`),
/// and the anchor (`anchor`, else [`default_anchor`]) with every commit
/// after it up to the base that changed the build.
fn sides(root: &Path, base: &str, anchor: Option<&str>) -> Result<Vec<Side>, String> {
    let sha = |rev: &str| {
        git(
            root,
            &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
        )
    };
    let short = |sha: &str| sha[..sha.len().min(10)].to_owned();
    let base_sha = git(root, &["merge-base", base, "HEAD"])?;
    let anchor = match anchor {
        Some(rev) => Some((rev.to_owned(), sha(rev)?)),
        None => default_anchor(root, &base_sha, ACCEPTED, "v8-ir")?,
    }
    .filter(|(_, a)| *a != base_sha);
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
            name: format!("{base} ({})", short(&base_sha)),
            sha: Some(base_sha.clone()),
        },
    ];
    if let Some((rev, a)) = &anchor {
        sides.push(Side {
            label: "anchor".to_owned(),
            tree: "anchor",
            name: format!("{rev} ({})", short(a)),
            sha: Some(a.clone()),
        });
        for c in changed_since(root, a, &base_sha)? {
            sides.push(Side {
                label: format!("since-{}", short(&c)),
                tree: "anchor",
                name: format!("{} since the anchor", short(&c)),
                sha: Some(c),
            });
        }
    }
    Ok(sides)
}

/// The JS the count runs: the driver and the host it loads.
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
    let scratch = dir.join("callgrind");
    for d in [&cache, &store, &scratch] {
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
                        &js,
                        &store.join(format!("{id}.wasm")),
                        &bench.join(format!("{stem}.py")),
                        expected.get(name).map_or("", String::as_str),
                        &scratch.join(format!("{id}.{stem}")),
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
            let tree = dir.join(format!("{}-src", side.tree));
            let own = size::builds_itself(root, sha);
            if own {
                checkout(root, &tree, sha)?;
            }
            (tree, own)
        }
        None => (root.to_path_buf(), true),
    };
    let recipe = size::recipe_key(if own { &tree } else { root })?;
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
        for (guest, path) in guests(root, &dir, side, &tree, own, head_built)? {
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
    dir: &Path,
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
                    "v8-ir: building the {} with the head's recipe (its xtask has no `size --head`)",
                    side.label
                );
                Ok(listed(crate::shipped_guests(tree, &dir.join(side.tree))?))
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

/// One program's second run on `guest` under callgrind: V8's generated
/// code's instructions. The dumps are removed once read.
fn count(
    valgrind: &Path,
    node: &Path,
    js: &Path,
    guest: &Path,
    program: &Path,
    want: &str,
    prefix: &Path,
) -> Result<u64, String> {
    let output = Command::new(valgrind)
        .args(["--tool=callgrind", "--dump-after=uv_getrusage"])
        .arg(format!("--callgrind-out-file={}.%p.cg", prefix.display()))
        .arg(node)
        .args(["--no-liftoff", "bench/count.ts"])
        .arg(guest)
        .arg(format!("{}={want}", program.display()))
        .current_dir(js)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {}: {e}", valgrind.display()))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let what = format!("{} on {}", program.display(), guest.display());
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
    let base = format!("{}.{pid}.cg", prefix.display());
    // Dump 1: start-up and the module's compile; 2: the first run; 3: the
    // second, counted.
    let dump = PathBuf::from(format!("{base}.3"));
    let counted = fs::read_to_string(&dump)
        .map_err(|e| format!("{what}: {}: {e}", dump.display()))
        .map(|cg| generated(&cg));
    for n in ["", ".1", ".2", ".3"] {
        let _ = fs::remove_file(format!("{base}{n}"));
    }
    match counted? {
        0 => Err(format!("{what}: no instructions in generated code")),
        n => Ok(n),
    }
}

/// The self instructions in `cg` (one callgrind dump) at code with no
/// object file, which callgrind names `???`. Names are compressed:
/// `ob=(N) name` defines `N`, a later `ob=(N)` refers to it. The cost line
/// after a `calls=` line is the call's inclusive cost, not self cost.
fn generated(cg: &str) -> u64 {
    let (mut unknown, mut ob, mut call, mut n) = (Vec::new(), "", false, 0_u64);
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
        let (Some(_), Some(ir)) = (f.next(), f.next()) else {
            continue;
        };
        if core::mem::take(&mut call) {
            continue;
        }
        if unknown.contains(&ob) {
            n += ir.parse::<u64>().unwrap_or(0);
        }
    }
    n
}

/// Each count's least over `counts`.
fn least(counts: &[Counts]) -> Counts {
    let mut out = Counts::new();
    for c in counts {
        for (id, &n) in c {
            out.entry(id.clone())
                .and_modify(|m: &mut u64| *m = (*m).min(n))
                .or_insert(n);
        }
    }
    out
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
/// anchor.
fn report(
    guest: &str,
    programs: &[(&str, &str)],
    head: &Counts,
    base: (&str, &Counts),
    anchor: Option<(&str, &Counts)>,
) -> Vec<String> {
    let at = |c: &Counts, name: &str| c.get(&format!("{guest}/{name}")).copied().unwrap_or(0);
    println!("\nV8 instructions in generated code, {guest}, second run, millions");
    let mut header = format!(
        "  {:<11} {:>10} {:>10} {:>10}",
        "program", "base", "head", "head/base"
    );
    if anchor.is_some() {
        let _ = write!(header, " {:>10} {:>11}", "anchor", "head/anchor");
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
        if let Some((_, a)) = anchor {
            let a = at(a, name);
            let _ = write!(line, " {:>10.3} {:>11}", millions(a), show(ratio(h, a)));
        }
        println!("{line}");
    }
    let mut failures = Vec::new();
    let mut geo = String::from("  geomean");
    for (against, other) in [Some(base), anchor].into_iter().flatten() {
        let (mut product, mut n) = (1.0_f64, 0_i32);
        for &(name, _) in programs {
            let r = ratio(at(head, name), at(other, name));
            product *= r;
            n += 1;
            if r > PROGRAM_LIMIT {
                failures.push(format!("{guest} {name} {r:.4} against {against}"));
            }
        }
        let geomean = product.powf(1.0 / f64::from(n.max(1)));
        let _ = write!(geo, " {geomean:.4} against {against};");
        if geomean > GEOMEAN_LIMIT {
            failures.push(format!("{guest} geomean {geomean:.4} against {against}"));
        }
    }
    println!("{geo}\n  (^ past +0.5%, v below -0.5%; the geomean's limit is +0.3%)");
    failures
}

/// Passes when nothing rose past a limit, or `MM_V8_IR_ACCEPT` says why.
fn verdict(failures: &[String]) -> Result<(), String> {
    let accepted = env::var("MM_V8_IR_ACCEPT").unwrap_or_default();
    if failures.is_empty() {
        println!("\nv8-ir: no count rose past its limit");
        Ok(())
    } else if !accepted.trim().is_empty() {
        println!(
            "\nv8-ir: past the limits ({}), accepted: {}",
            failures.join("; "),
            accepted.trim()
        );
        Ok(())
    } else {
        Err(format!(
            "v8-ir: past the limits: {}; if the rise is intended, rerun with \
             MM_V8_IR_ACCEPT=<reason> and put the table and a `{ACCEPTED} <reason>` \
             line in the commit message (later runs anchor there)",
            failures.join("; ")
        ))
    }
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
    use super::generated;

    #[test]
    fn counts_self_cost_in_code_without_an_object_file() {
        let cg = "\
ob=(1) /usr/bin/node
fn=(1) main
0x10 5
ob=(2) ???
fn=(2) 0x7f00
0x20 7
+1 3
cob=(1)
cfn=(1)
calls=1 0x10
0x21 1000
ob=(1)
fn=(1)
0x11 11
ob=(2)
fn=(2)
* 2
";
        // 7 + 3 + 2 in `???`; the call's 1000 is inclusive cost, node's own
        // 5 and 11 are not generated code.
        assert_eq!(generated(cg), 12);
    }
}
