//! `cargo xtask wasm-ir`: wasmtime's guest instructions on the bench
//! programs against a base revision and an anchor.
//!
//! Builds the working tree (or `--head`), `--base` (default `origin/main`)
//! and the anchor, each revision checked out in a worktree outside this
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
//! A committed revision's counts are cached in `target/wasm-ir/cache/`,
//! keyed by the counting method and the wasmtime, `wasm-opt` and valgrind
//! versions, each program's count also by its source's hash (the programs
//! are this checkout's `bench/`, whatever the revision), so an unmoved base
//! or anchor is neither built nor run again, and an edited program is.
//!
//! The anchor is the latest `v*` tag the base descends from, or, when a
//! commit since that tag accepted a rise (a `Wasm-IR-Accepted:` line in its
//! message), the latest such commit; `--anchor REV` names another. A
//! checkout without the tag fails rather than go without an anchor. The
//! anchor's count of a program is the least of the anchor's, the base's
//! and every first-parent commit's between them that changed the build's
//! sources (`INPUTS`), so a gain moves the anchor down with it and later
//! rises count from there. Against both base and anchor, the gate fails when a program's count rises past
//! 1.0% (`PROGRAM_LIMIT`), the geometric mean of the ratios past 0.5%
//! (`GEOMEAN_LIMIT`), or `startup`'s count, about 0.9M, by more than
//! `STARTUP_LIMIT` instructions; `startup` is left out of the geomean.
//! Unrelated commits move the run loop's register allocation by up to
//! about 1% either way; the anchor bounds a creep of commits each under the
//! limits. Fewer instructions always pass, with the table printed.
//! `MM_WASM_IR_ACCEPT=<reason>` passes an intended rise, whose commit
//! message records the table and the reason on a `Wasm-IR-Accepted:` line.
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
    bench::{PROGRAMS, read_expected},
    wasm_wall::{build_mm, checkout, git, head_id},
    workspace_root,
};

/// The most one program's count may rise.
const PROGRAM_LIMIT: f64 = 1.010;
/// The most the geometric mean of the ratios (without `startup`) may rise.
const GEOMEAN_LIMIT: f64 = 1.005;
/// The most `startup`'s count may rise, in instructions: it is about 0.9M,
/// where 1% is one new table entry.
const STARTUP_LIMIT: u64 = 50_000;
/// What a count holds, in the cache key: change it when that changes.
const COUNTING: &str = "guest code and memory.copy/fill libcalls, 1";
/// wasmtime's libcalls for `memory.copy` and `memory.fill`.
const BULK: [&str; 2] = [
    "wasmtime::runtime::vm::libcalls::raw::memory_copy",
    "wasmtime::runtime::vm::libcalls::raw::memory_fill",
];
/// The line that marks a commit whose rise was accepted; it re-anchors.
const ACCEPTED: &str = "Wasm-IR-Accepted:";
/// What a revision's `mm` is built from (the running xtask builds every
/// revision, so its flags are this checkout's): a commit that changes none
/// of it counts as its parent does.
const INPUTS: [&str; 8] = [
    "crates",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo",
    ":(exclude)crates/*/tests/*",
    ":(exclude)crates/mm-wasm-host",
    ":(exclude)*.md",
];

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

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut base = "origin/main".to_owned();
    let (mut head, mut anchor, mut only) = (None::<String>, None::<String>, None::<String>);
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
            "--anchor" => anchor = Some(value("a revision")?),
            "--programs" => only = Some(value("a list")?),
            other => return Err(format!("wasm-ir: unknown option `{other}`")),
        }
    }
    let programs = select(only.as_deref())?;
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
    let key = [
        COUNTING.to_owned(),
        isa.clone(),
        version(&wasmtime),
        version(&wasm_opt),
        version(&valgrind),
    ]
    .join("; ");
    let root = workspace_root();
    let sha = |rev: &str| {
        git(
            &root,
            &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
        )
    };
    let short = |sha: &str| sha[..sha.len().min(10)].to_owned();

    let base_sha = sha(&base)?;
    let anchor = match &anchor {
        Some(rev) => Some((rev.clone(), sha(rev)?)),
        None => default_anchor(&root, &base_sha, ACCEPTED, "wasm-ir")?,
    }
    .filter(|(_, a)| *a != base_sha);
    let mut sides = vec![
        match &head {
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
        },
        Side {
            label: "base".to_owned(),
            target: "base",
            name: format!("{base} ({})", short(&base_sha)),
            sha: Some(base_sha.clone()),
        },
    ];
    if let Some((rev, a)) = &anchor {
        sides.extend(anchor_sides(&root, rev, a, &base_sha)?);
    }
    for side in &sides {
        println!("wasm-ir: {:<6} = {}", side.label, side.name);
    }

    let tools = Tools {
        valgrind,
        wasmtime,
        isa,
        key,
    };
    let counts = measure(&root, &tools, &sides, &programs, head.is_some())?;
    // The anchor's counts: the least from the anchor to the base.
    let least = (sides.len() > 2).then(|| least(&counts[1..]));
    let anchor_name = sides
        .get(2)
        .map(|side| format!("{} to the base", side.name));
    let anchor = anchor_name.as_deref().zip(least.as_ref());
    let failures = report(&programs, &counts[0], (&sides[1].name, &counts[1]), anchor);
    verdict(&failures)
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

/// The anchor `rev` (commit `sha`) and the commits after it up to `base`
/// that changed the build, as sides to count.
fn anchor_sides(root: &Path, rev: &str, sha: &str, base: &str) -> Result<Vec<Side>, String> {
    let short = |sha: &str| sha[..sha.len().min(10)].to_owned();
    let mut sides = vec![Side {
        label: "anchor".to_owned(),
        target: "anchor",
        name: format!("{rev} ({})", short(sha)),
        sha: Some(sha.to_owned()),
    }];
    for c in changed_since(root, sha, base)? {
        sides.push(Side {
            label: format!("since-{}", short(&c)),
            target: "anchor",
            name: format!("{} since the anchor", short(&c)),
            sha: Some(c),
        });
    }
    Ok(sides)
}

/// Each program's least count over `counts`.
fn least(counts: &[Counts]) -> Counts {
    let mut out = Counts::new();
    for c in counts {
        for (name, &n) in c {
            out.entry(name.clone())
                .and_modify(|m: &mut Count| {
                    if n.total() < m.total() {
                        *m = n;
                    }
                })
                .or_insert(n);
        }
    }
    out
}

/// The first-parent commits after `anchor` up to `base`, `base` left out,
/// whose `INPUTS` differ from the commit before them.
pub(crate) fn changed_since(root: &Path, anchor: &str, base: &str) -> Result<Vec<String>, String> {
    let list = git(
        root,
        &[
            "rev-list",
            "--first-parent",
            "--reverse",
            &format!("{anchor}..{base}"),
        ],
    )?;
    let mut out = Vec::new();
    let mut before = anchor.to_owned();
    for sha in list.lines() {
        let args = [
            &["diff", "--name-only", before.as_str(), sha, "--"][..],
            &INPUTS,
        ]
        .concat();
        if sha != base && !git(root, &args)?.is_empty() {
            out.push(sha.to_owned());
        }
        sha.clone_into(&mut before);
    }
    Ok(out)
}

/// FNV-1a of a bench program's source, which keys its cached counts.
pub(crate) fn source_hash(bytes: &[u8]) -> String {
    let h = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{h:016x}")
}

/// The tools a count runs, and the cache key they make.
struct Tools {
    valgrind: PathBuf,
    wasmtime: PathBuf,
    isa: String,
    key: String,
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
    let mut jobs: Vec<(usize, &str, &str, PathBuf)> = Vec::new();
    for (i, side) in sides.iter().enumerate() {
        let cached = side
            .sha
            .as_deref()
            .map_or_default(|s| load(&cache, s, &tools.key, &hashes));
        let missing: Vec<(&str, &str)> = programs
            .iter()
            .copied()
            .filter(|(name, _)| !cached.contains_key(*name))
            .collect();
        if missing.is_empty() {
            println!("wasm-ir: {} counts cached", side.label);
        } else {
            // Another revision is checked out and built outside this
            // checkout, so it reads none of this checkout's `.cargo/`.
            let refs = crate::shipped::refs_dir("wasm-ir");
            let (tree, target) = match &side.sha {
                Some(s) if side.target != "head" || head_rev => {
                    let tree = refs.join(format!("{}-src", side.target));
                    checkout(root, &tree, s)?;
                    (tree, refs.join(side.target))
                }
                _ => (root.to_path_buf(), dir.join(side.target)),
            };
            let built = build_mm(&tree, &target, &tools.wasmtime, Some(&tools.isa))?;
            // Sides share a target directory: each keeps its own module.
            let mm = dir.join(format!("{}.cwasm", side.label));
            fs::copy(&built, &mm).map_err(|error| format!("{}: {error}", mm.display()))?;
            jobs.extend(missing.into_iter().map(|(n, s)| (i, n, s, mm.clone())));
        }
        counts.push(cached);
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
    for (side, c) in sides.iter().zip(&counts) {
        if let Some(s) = &side.sha {
            save(&cache, s, &tools.key, &hashes, c)?;
        }
    }
    Ok(counts)
}

/// The latest `v*` tag `base` descends from, or the latest commit since it
/// whose message has an `accepted` line (a rise accepted); an error,
/// naming `gate`, without such a tag.
pub(crate) fn default_anchor(
    root: &Path,
    base: &str,
    accepted: &str,
    gate: &str,
) -> Result<Option<(String, String)>, String> {
    let Ok(tag) = git(
        root,
        &["describe", "--tags", "--abbrev=0", "--match", "v*", base],
    ) else {
        return Err(format!(
            "{gate}: no v* tag reachable from {base}, so no anchor: fetch the \
             tags (`git fetch --tags origin`) or name one with --anchor REV"
        ));
    };
    let accepted = git(
        root,
        &[
            "log",
            "-1",
            "--format=%H",
            &format!("--grep=^{accepted}"),
            &format!("{tag}..{base}"),
        ],
    )?;
    if accepted.is_empty() {
        let sha = git(
            root,
            &["rev-parse", "--verify", &format!("{tag}^{{commit}}")],
        )?;
        Ok(Some((tag, sha)))
    } else {
        Ok(Some((format!("accepted since {tag}"), accepted)))
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
    let Ok(text) = fs::read_to_string(cache.join(format!("{sha}.tsv"))) else {
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

/// Passes when nothing rose past a limit, or `MM_WASM_IR_ACCEPT` says why
/// it did.
fn verdict(failures: &[String]) -> Result<(), String> {
    let accepted = env::var("MM_WASM_IR_ACCEPT").unwrap_or_default();
    if failures.is_empty() {
        println!("\nwasm-ir: within the limits");
        Ok(())
    } else if !accepted.trim().is_empty() {
        println!(
            "\nwasm-ir: past the limits ({}), accepted: {}",
            failures.join("; "),
            accepted.trim()
        );
        Ok(())
    } else {
        Err(format!(
            "wasm-ir: more guest instructions: {}; if the rise is intended, \
             rerun with MM_WASM_IR_ACCEPT=<reason> and put the table and a \
             `{ACCEPTED} <reason>` line in the commit message (later runs \
             anchor on that commit)",
            failures.join("; ")
        ))
    }
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

/// Prints the table; returns the limits exceeded against base and anchor.
fn report(
    programs: &[(&str, &str)],
    head: &Counts,
    base: (&str, &Counts),
    anchor: Option<(&str, &Counts)>,
) -> Vec<String> {
    let at = |c: &Counts, name: &str| c.get(name).copied().unwrap_or_default();
    println!(
        "\nwasmtime instructions, guest code and memory.copy/fill, millions; \
         bulk = head's memory.copy/fill share"
    );
    let mut header = format!(
        "  {:<11} {:>10} {:>10} {:>9} {:>6}",
        "program", "base", "head", "head/base", "bulk"
    );
    if anchor.is_some() {
        let _ = write!(header, " {:>10} {:>11}", "anchor", "head/anchor");
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
        if let Some((_, a)) = anchor {
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
    let (geo_base, mut failures) = judge(programs, head, base, anchor.map(|a| a.1));
    let mut geo = format!("  geomean (without startup) head/base {geo_base:.4}");
    if let Some(a) = anchor {
        let (geo_anchor, more) = judge(programs, head, a, None);
        let _ = write!(geo, ", head/anchor {geo_anchor:.4}");
        failures.extend(more);
    }
    println!(
        "{geo}\n  (^ past +1.0%, v below -1.0%; startup in instructions, limit +{STARTUP_LIMIT})"
    );
    failures
}

/// The geomean of `head`'s ratios to `other` without `startup`, and the
/// limits it exceeds. A rise against the base that only returns to the
/// anchor's level (`back`) still fails, and says so.
fn judge(
    programs: &[(&str, &str)],
    head: &Counts,
    (against, other): (&str, &Counts),
    back: Option<&Counts>,
) -> (f64, Vec<String>) {
    let total = |c: &Counts, name: &str| c.get(name).map_or(0, |c| c.total());
    let mut failures = Vec::new();
    let (mut product, mut n) = (1.0_f64, 0_i32);
    for &(name, _) in programs {
        let (h, o) = (total(head, name), total(other, name));
        if name == "startup" {
            if h > o + STARTUP_LIMIT {
                failures.push(format!("startup +{} against {against}", h - o));
            }
            continue;
        }
        let r = ratio(h, o);
        product *= r;
        n += 1;
        if r > PROGRAM_LIMIT {
            let note = back
                .map(|a| ratio(h, total(a, name)))
                .filter(|&ra| ra <= PROGRAM_LIMIT)
                .map_or(String::new(), |ra| {
                    format!(" (head/anchor {ra:.4}: back to the anchor's level)")
                });
            failures.push(format!("{name} {r:.4} against {against}{note}"));
        }
    }
    let geomean = product.powf(1.0 / f64::from(n.max(1)));
    if geomean > GEOMEAN_LIMIT {
        failures.push(format!("geomean {geomean:.4} against {against}"));
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

    /// A new git repository under the temporary directory.
    fn repo(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("wasm-ir-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "--quiet"]).unwrap();
        dir
    }

    /// Writes `file` and commits it; the commit's id.
    fn commit(dir: &Path, file: &str) -> String {
        let path = dir.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, git(dir, &["rev-list", "--all"]).unwrap_or_default()).unwrap();
        git(dir, &["add", "--all"]).unwrap();
        let who = [
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "core.hooksPath=/dev/null",
        ];
        git(
            dir,
            &[&who[..], &["commit", "--quiet", "-m", file]].concat(),
        )
        .unwrap();
        git(dir, &["rev-parse", "HEAD"]).unwrap()
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

    #[test]
    fn a_checkout_without_the_tag_has_no_anchor() {
        let dir = repo("tags");
        let first = commit(&dir, "crates/a.rs");
        let error = default_anchor(&dir, &first, ACCEPTED, "wasm-ir").unwrap_err();
        assert!(error.contains("no v* tag"), "{error}");
        git(&dir, &["tag", "v0.0.1"]).unwrap();
        let head = commit(&dir, "crates/b.rs");
        assert_eq!(
            default_anchor(&dir, &head, ACCEPTED, "wasm-ir").unwrap(),
            Some(("v0.0.1".to_owned(), first))
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_anchor_counts_every_build_since_it() {
        let dir = repo("chain");
        let tag = commit(&dir, "crates/a.rs");
        commit(&dir, "docs/a.md");
        let changed = commit(&dir, "crates/b.rs");
        commit(&dir, "crates/x/tests/t.rs");
        commit(&dir, "crates/x/README.md");
        let base = commit(&dir, "Cargo.lock");
        assert_eq!(changed_since(&dir, &tag, &base).unwrap(), vec![changed]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn creep_after_a_gain_fails_against_the_anchor() {
        let programs = [("fib", "fib")];
        // The tag; a 6% gain; then base and head each rise under the limits.
        let (tag, gain, base, head) = (counts(1000), counts(940), counts(948), counts(952));
        let fails = |other: &Counts| !judge(&programs, &head, ("x", other), None).1.is_empty();
        assert!(
            !fails(&base) && !fails(&tag),
            "each step is under the limits"
        );
        assert!(fails(&least(&[base, tag, gain])), "1.3% over the gain");
    }
}
