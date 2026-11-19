//! The Ir gates' floors (`native-ir`, `wasm-ir`, `v8-ir`): for each count,
//! the lowest it has been, kept in the tree in `xtask/ir-floors/<gate>.txt`
//! (owner's decision, 2026-11-14; the 0.0.2 performance review, section
//! 17), so a gate compares the head with the base and with the floor and
//! builds no history.
//!
//! A line is `<name>\t<count>\t<build>\t<key>`:
//! - `<build>`: the build that set the floor, a hash of the blob ids of
//!   what a shipped build reads ([`build_id`]);
//! - `<key>`: what the count depends on besides the build: the gate's
//!   counting method, its tools' versions, the host's glibc and CPU class
//!   ([`host`]) and the program's source. A floor whose key is not this
//!   run's is stale: the gate recounts the floor's build (found by its
//!   build id on the first-parent line since the last tag) instead of
//!   trusting it.
//!
//! Against the base's file ([`judge`]), floors bind upward only (owner's
//! decision, 2026-11-19):
//! - a count below its floor passes and leaves the file alone;
//!   `--write-floors` lowers it to the head's count when someone runs it,
//!   and each tag's seed does;
//! - a floor rises only to the head's count, and only with an accept line
//!   ([`crate::accept`]) that names the count with a ceiling at or above
//!   the rise over the floor;
//! - a new count needs a line at or below the head's count once;
//! - any other floor stays as the base has it.
//!
//! So a drop followed by a rise back to the old floor passes until the
//! floors are next lowered; a geomean against the floors takes the base's
//! count where it is lower ([`lower`]), so the drop is no headroom for
//! other counts. `--write-floors` writes that file. Against a
//! `v*` tag as the base (`just pretag`), the head's floors are checked
//! against the tag's counts instead: none above the tag's count without a
//! line since the tag. There `--write-floors` seeds the file
//! from the tag ([`seed`]): each floor is the tag's count with the tag's
//! build, or the head's where the head counts less, whatever the file held
//! before, so a release starts its floors at its own counts (`just
//! seed-floors`, the first commit after a tag).
//!
//! Two landings that both change a floor conflict on its line. Take the
//! raise when one side is an accepted raise, else the lower count.

use std::{collections::BTreeMap, env, fmt::Write as _, fs, path::Path, process::Command};

use crate::{wasm_ir::source_hash, wasm_wall::git};

/// Where the floor files are, one per gate.
const DIR: &str = "xtask/ir-floors";

/// What a shipped build reads ([`build_id`]): under `crates/`, all but
/// `tests/`; with the recipe.
const BUILD: [&str; 7] = [
    "crates",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo",
    "xtask/src/shipped.rs",
    "xtask/src/wasm.rs",
];

/// One count's floor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Floor {
    pub(crate) count: u64,
    pub(crate) build: String,
    pub(crate) key: String,
}

/// Floors by count name (`dict`, `plain/dict`).
pub(crate) type Floors = BTreeMap<String, Floor>;

/// Counts by name.
pub(crate) type Counts = BTreeMap<String, u64>;

fn file(gate: &str) -> String {
    format!("{DIR}/{gate}.txt")
}

fn parse(gate: &str, text: &str) -> Result<Floors, String> {
    let mut out = Floors::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let [name, count, build, key] = f[..] else {
            return Err(format!(
                "{}: `{line}` is not `name count build key`",
                file(gate)
            ));
        };
        let count = count
            .parse()
            .map_err(|_| format!("{}: `{count}` in `{line}` is not a count", file(gate)))?;
        let floor = Floor {
            count,
            build: build.to_owned(),
            key: key.to_owned(),
        };
        if out.insert(name.to_owned(), floor).is_some() {
            return Err(format!("{}: two lines for `{name}`", file(gate)));
        }
    }
    Ok(out)
}

/// The floors of `gate` as commit `rev` holds them (none without a file).
pub(crate) fn at(root: &Path, rev: &str, gate: &str) -> Result<Floors, String> {
    match git(root, &["show", &format!("{rev}:{}", file(gate))]) {
        Ok(text) => parse(gate, &text),
        Err(_) => Ok(Floors::new()),
    }
}

/// The floors of `gate` in the working tree (none without a file).
pub(crate) fn here(root: &Path, gate: &str) -> Result<Floors, String> {
    match fs::read_to_string(root.join(file(gate))) {
        Ok(text) => parse(gate, &text),
        Err(_) => Ok(Floors::new()),
    }
}

fn write(root: &Path, gate: &str, floors: &Floors) -> Result<(), String> {
    let path = root.join(file(gate));
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut text = format!(
        "# {gate}: each count's floor, the lowest it has been (xtask/src/floors.rs).\n\
         # Written by `cargo xtask {gate} --write-floors`; a floor rises only with an\n\
         # accept line. name\\tcount\\tbuild\\tkey\n"
    );
    for (name, f) in floors {
        let _ = writeln!(text, "{name}\t{}\t{}\t{}", f.count, f.build, f.key);
    }
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// What a count depends on in the host: glibc's version (its `memcpy`
/// and friends are counted) and the CPU class it picks them by under
/// `valgrind`, which hides AVX-512 (`x86-64-v3` on a v4 machine [M]).
pub(crate) fn host(valgrind: &Path) -> String {
    let glibc = Command::new("getconf")
        .arg("GNU_LIBC_VERSION")
        .output()
        .map_or_default(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let class = Command::new(valgrind)
        .args(["-q", "/lib64/ld-linux-x86-64.so.2", "--help"])
        .output()
        .map_or_default(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .find(|l| l.contains("(supported, searched)"))
                .and_then(|l| l.split_whitespace().next())
                .unwrap_or_default()
                .to_owned()
        });
    format!(
        "{glibc}, {}",
        if class.is_empty() { "x86-64" } else { &class }
    )
}

/// A count's key: the gate's `method` (counting, tools, host) and the
/// program's source hash.
pub(crate) fn key(method: &str, source: &str) -> String {
    format!(
        "k-{}",
        source_hash(format!("{method}; {source}").as_bytes())
    )
}

/// A name for the build of `rev`, or of the working tree: a hash of the
/// blob ids of [`BUILD`] (no `tests/`).
pub(crate) fn build_id(root: &Path, rev: Option<&str>) -> Result<String, String> {
    let tree = if let Some(rev) = rev {
        rev.to_owned()
    } else {
        {
            // The working tree as a tree object, through a scratch index.
            let index = env::temp_dir().join(format!("mm-floors-{}.index", std::process::id()));
            let run = |args: &[&str]| -> Result<String, String> {
                let output = Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .env("GIT_INDEX_FILE", &index)
                    .output()
                    .map_err(|e| format!("cannot run git: {e}"))?;
                if !output.status.success() {
                    return Err(format!(
                        "git {}: {}",
                        args.join(" "),
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
            };
            let tree = run(&["read-tree", "HEAD"])
                .and_then(|_| run(&[&["add", "--all", "--"][..], &BUILD].concat()))
                .and_then(|_| run(&["write-tree"]));
            let _ = fs::remove_file(&index);
            tree?
        }
    };
    let listing = git(
        root,
        &[&["ls-tree", "-r", &tree, "--"][..], &BUILD].concat(),
    )?;
    let read: Vec<&str> = listing
        .lines()
        .filter(|line| {
            !line
                .split('\t')
                .nth(1)
                .unwrap_or_default()
                .contains("/tests/")
        })
        .collect();
    Ok(format!("b-{}", source_hash(read.join("\n").as_bytes())))
}

/// The build of each commit on the first-parent line from the last `v*`
/// tag that `from` descends from (`from` itself when it is one) to `to`,
/// newest first, as (build, commit); and that tag.
fn builds(root: &Path, from: &str, to: &str) -> Result<(Vec<(String, String)>, String), String> {
    let tag = git(
        root,
        &["describe", "--tags", "--abbrev=0", "--match", "v*", from],
    )
    .ok();
    // `<tag>^@`: the tag's parents, none for a root commit.
    let not = tag.as_ref().map(|t| format!("{t}^@"));
    let mut args = vec!["rev-list", "--first-parent", to];
    if let Some(not) = &not {
        args.extend(["--not", not]);
    }
    let mut out = Vec::new();
    for sha in git(root, &args)?.lines() {
        out.push((build_id(root, Some(sha))?, sha.to_owned()));
    }
    Ok((out, tag.unwrap_or_else(|| "the root".to_owned())))
}

/// The floors `gate` starts from (the base's, or at a tag the working
/// tree's) whose key is not `keys`' (stale): the commit of each one's
/// build, which the gate recounts, and the names it recounts. A build is
/// looked for on the first-parent line since the last tag, up to the base
/// (up to HEAD at a tag, where the floors are the head's). None when
/// `write` seeds the floors at a tag.
pub(crate) fn stale(
    root: &Path,
    gate: &str,
    base: &str,
    tag: bool,
    write: bool,
    keys: &BTreeMap<String, String>,
) -> Result<Vec<(String, Vec<String>)>, String> {
    // Seeding at a tag replaces every floor it counts; none is recounted.
    if tag && write {
        return Ok(Vec::new());
    }
    let floors = if tag {
        here(root, gate)?
    } else {
        at(root, base, gate)?
    };
    let mut wanted: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (name, f) in &floors {
        if keys.get(name).is_some_and(|k| *k != f.key) {
            wanted.entry(&f.build).or_default().push(name.clone());
        }
    }
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let (line, since) = builds(root, base, if tag { "HEAD" } else { base })?;
    // replay-ir has two floors per session: name a few and count the rest.
    let some = |names: &[String]| match names {
        [a, b, c, _, ..] => format!("{a}, {b}, {c} and {} more", names.len() - 3),
        _ => names.join(", "),
    };
    let mut out = Vec::new();
    for (build, names) in wanted {
        let Some((_, sha)) = line.iter().find(|(b, _)| b == build) else {
            return Err(format!(
                "{gate}: the floors of {} are stale (another counting method, tool, host or \
                 program source) and no commit since {since} has their build {build}, so they \
                 cannot be recounted. A tag starts new floors: seed them from it with `just \
                 seed-floors {since}` as the first commit after it, and judge that commit with \
                 `cargo xtask check --base {since}`",
                some(&names)
            ));
        };
        println!(
            "{gate}: the floors of {} are stale: recounting their build, {}",
            some(&names),
            &sha[..sha.len().min(10)]
        );
        out.push((sha.clone(), names));
    }
    Ok(out)
}

/// Each stale floor's recount: from `counts`, the counts of the builds
/// [`stale`] returned, in its order.
pub(crate) fn recounted(stale: &[(String, Vec<String>)], counts: &[Counts]) -> Counts {
    let mut out = Counts::new();
    for ((_, names), c) in stale.iter().zip(counts) {
        for name in names {
            if let Some(&n) = c.get(name) {
                out.insert(name.clone(), n);
            }
        }
    }
    out
}

/// A gate's verdict on its counts, with what is wrong with its floors.
pub(crate) fn verdict(counts: Result<(), String>, errors: &[String]) -> Result<(), String> {
    if errors.is_empty() {
        return counts;
    }
    Err([counts.err().unwrap_or_default(), errors.join("\n")]
        .join("\n")
        .trim()
        .to_owned())
}

/// What `judge` is given about the head and the base.
pub(crate) struct Sides<'a> {
    /// The base commit, and whether it is a `v*` tag (the pre-tag check).
    pub(crate) base: &'a str,
    pub(crate) tag: bool,
    pub(crate) head: &'a Counts,
    pub(crate) base_counts: &'a Counts,
    /// This run's key of each counted name.
    pub(crate) keys: &'a BTreeMap<String, String>,
    pub(crate) head_build: &'a str,
    /// The recounted value of each stale floor ([`stale`]).
    pub(crate) recounted: &'a Counts,
}

/// How a gate judges a count against another (the base's, the floor or
/// the tag's), for [`judge`].
pub(crate) struct Rules<'a> {
    /// An accept line since the base names `name` with a ceiling at or
    /// above `ratio`.
    pub(crate) accepted: &'a dyn Fn(&str, f64) -> bool,
    /// `name`'s count `head` is past the gate's limit over `other`.
    pub(crate) past: &'a dyn Fn(&str, u64, u64) -> bool,
    /// `name` is part of a geomean that is past its limit against the
    /// floors and that an accept line covers ([`geomean`]): its count
    /// raises its floor wherever it rose, past its own limit or not, or
    /// the next landing fails on the same geomean.
    pub(crate) swept: &'a dyn Fn(&str) -> bool,
    /// How far `name`'s count of one build may read below a floor `f` of
    /// it and still be the same count: 0 for an exact count ([`exact`]);
    /// replay-ir's sessions race inside the worker.
    pub(crate) band: fn(&str, u64) -> u64,
}

/// The geomean of `head`'s ratios to `floors` over the names `of` takes
/// that both hold, for [`Rules::swept`]: each against the lower of its
/// floor and `base`'s count ([`lower`]).
#[allow(clippy::cast_precision_loss, reason = "a ratio of counts")]
pub(crate) fn geomean(
    head: &Counts,
    floors: &Counts,
    base: &Counts,
    of: &dyn Fn(&str) -> bool,
) -> f64 {
    let (mut product, mut n) = (1.0_f64, 0_i32);
    for (name, &h) in head.iter().filter(|(name, _)| of(name)) {
        if let Some(&f) = floors.get(name) {
            product *= ratio(h, lower(f, base.get(name).copied()));
            n += 1;
        }
    }
    product.powf(1.0 / f64::from(n.max(1)))
}

/// What a geomean (or a median) against the floors takes for a count with
/// floor `f`: the base's count where it is lower. A floor left above a
/// count that dropped binds that count upward only; it is no headroom for
/// the other counts' rises.
pub(crate) fn lower(f: u64, base: Option<u64>) -> u64 {
    base.map_or(f, |b| f.min(b))
}

/// [`Rules::band`] of a count that repeats exactly.
pub(crate) fn exact(_: &str, _: u64) -> u64 {
    0
}

impl Rules<'_> {
    /// Whether the head's count `h` raises a floor `f`: a rise past the
    /// limit that an accept line covers, so a line that lists more names
    /// than rose leaves their floors where they are; or any rise of a
    /// count in an accepted geomean ([`Rules::swept`]).
    fn raises(&self, name: &str, h: u64, f: u64) -> bool {
        h > f
            && ((self.swept)(name) || (self.past)(name, h, f) && (self.accepted)(name, ratio(h, f)))
    }

    /// Whether the head's count `h` lowers a floor `f`: below it by more
    /// than the band.
    fn lowers(&self, name: &str, h: u64, f: u64) -> bool {
        h.saturating_add((self.band)(name, f)) < f
    }

    /// Whether `a` and `b` are one count, within the band.
    fn same(&self, name: &str, a: u64, b: u64) -> bool {
        a.abs_diff(b) <= (self.band)(name, a.max(b))
    }
}

#[allow(clippy::cast_precision_loss, reason = "a ratio of counts")]
fn ratio(a: u64, b: u64) -> f64 {
    a as f64 / b.max(1) as f64
}

/// The floors as this run counts them: a stale floor's count and key are
/// its build's recount.
fn this_run(source: &Floors, sides: &Sides<'_>) -> Floors {
    let mut floors = source.clone();
    for (name, f) in &mut floors {
        if let (Some(&n), Some(k)) = (sides.recounted.get(name), sides.keys.get(name)) {
            f.count = n;
            f.key.clone_from(k);
        }
    }
    floors
}

/// The floors the head is measured against, and what is wrong with the
/// head's floor file (empty when nothing). With `write`, writes the file
/// the head must have first; at a tag, the file [`seed`] makes.
pub(crate) fn judge(
    root: &Path,
    gate: &str,
    sides: &Sides<'_>,
    rules: &Rules<'_>,
    write: bool,
) -> Result<(Counts, Vec<String>), String> {
    if sides.tag {
        if write {
            seed(root, gate, sides, rules)?;
        }
        return pre_tag(root, gate, sides, rules);
    }
    let source = at(root, sides.base, gate)?;
    let floors = this_run(&source, sides);
    let mut against: Counts = floors
        .iter()
        .filter(|(name, f)| sides.keys.get(*name) == Some(&f.key))
        .map(|(name, f)| (name.clone(), f.count))
        .collect();
    // A stale floor the gate could not recount is not a floor to measure
    // or replace.
    let mut errors = Vec::new();
    for name in sides.head.keys() {
        if let (Some(f), Some(k)) = (floors.get(name), sides.keys.get(name))
            && f.key != *k
        {
            errors.push(format!(
                "{gate}: {name}'s floor in the base's {} is stale (another counting method, \
                 tool, host or program source) and its build {} was not recounted",
                file(gate),
                f.build
            ));
        }
    }
    if !errors.is_empty() {
        return Ok((against, errors));
    }
    // The line the head's file must hold for each counted name, and why;
    // for a floor the head does not move, the base's line, which a stale
    // floor keeps (its recount is this run's, not the file's): a file
    // stays the same on every host until a count moves.
    let mut have = here(root, gate)?;
    let mut want: BTreeMap<String, (Floor, String)> = BTreeMap::new();
    for (name, &h) in sides.head {
        let Some(key) = sides.keys.get(name) else {
            continue;
        };
        let at_head = Floor {
            count: h,
            build: sides.head_build.to_owned(),
            key: key.clone(),
        };
        let held = have.get(name) == Some(&at_head);
        let (line, why) = match floors.get(name) {
            // A count the base has no floor for: the head's file may seed
            // it at or below the head's count, never above it by more than
            // the band (a racing count's seed is the lowest of its reads).
            None => {
                let seed = have
                    .get(name)
                    .filter(|f| !rules.lowers(name, h, f.count) && f.key == *key)
                    .cloned();
                let line = seed.unwrap_or(at_head);
                against.insert(name.clone(), line.count);
                (line, "a new count".to_owned())
            }
            // A drop leaves the floor as it is; `--write-floors` lowers it.
            Some(f) if rules.lowers(name, h, f.count) && (write || held) => {
                (at_head, String::new())
            }
            Some(f) if rules.raises(name, h, f.count) => {
                let why = format!(
                    "an accept line raises it from {} to the head's {h}",
                    f.count
                );
                (at_head, why)
            }
            Some(_) => {
                let why = if sides.recounted.contains_key(name) {
                    "the base's line: a stale floor is recounted, not rewritten"
                } else {
                    "only the head's count or an accept line moves a floor"
                };
                (source[name].clone(), why.to_owned())
            }
        };
        want.insert(name.clone(), (line, why));
    }
    let wrong: Floors = want
        .iter()
        .filter(|(name, (w, _))| have.get(*name) != Some(w))
        .map(|(name, (w, _))| (name.clone(), w.clone()))
        .collect();
    if write && !wrong.is_empty() {
        write_merged(root, gate, &mut have, &wrong)?;
        println!("{gate}: wrote {}", file(gate));
        return Ok((against, errors));
    }
    for (name, w) in &wrong {
        errors.push(format!(
            "{gate}: {} must hold `{name}\t{}\t{}\t{}` ({}){}",
            file(gate),
            w.count,
            w.build,
            w.key,
            want[name].1,
            have.get(name)
                .map_or_else(String::new, |h| format!(", not {}", h.count))
        ));
    }
    if !errors.is_empty() {
        errors.push(format!(
            "{gate}: `cargo xtask {gate} --write-floors` writes it; commit it with the change"
        ));
    }
    Ok((against, errors))
}

/// The pre-tag check, with the base a `v*` tag (`just pretag`, and the
/// first landing after a release): the head's own floors against the
/// tag's counts and the head's.
/// - Every counted line's build is on the first-parent line since the tag
///   (or is the head's), so a stale floor can always be recounted: after
///   a release the floors must be seeded from it (`just seed-floors`).
/// - A line on the tag's build holds the tag's count and one on the head's
///   build the head's, so a floor cannot be lowered or relabelled by hand.
/// - No floor is above the tag's count without an accept line since the
///   tag; and a rise past the limit that a line accepts has raised its
///   floor to the head's count, as a landing does. A floor above the
///   head's count passes, as in a landing.
fn pre_tag(
    root: &Path,
    gate: &str,
    sides: &Sides<'_>,
    rules: &Rules<'_>,
) -> Result<(Counts, Vec<String>), String> {
    let source = here(root, gate)?;
    let floors = this_run(&source, sides);
    let against: Counts = floors
        .iter()
        .filter(|(name, f)| sides.keys.get(*name) == Some(&f.key))
        .map(|(name, f)| (name.clone(), f.count))
        .collect();
    let (line, tag) = builds(root, sides.base, "HEAD")?;
    let tag_build = build_id(root, Some(sides.base))?;
    let seed = format!("seed the floors from the tag: `just seed-floors {tag}`");
    let mut errors = Vec::new();
    for (name, &h) in sides.head {
        let Some(s) = source.get(name) else {
            errors.push(format!(
                "{gate}: no floor for {name} in {}; {seed}",
                file(gate)
            ));
            continue;
        };
        if s.build != sides.head_build && !line.iter().any(|(b, _)| *b == s.build) {
            errors.push(format!(
                "{gate}: {name}'s floor is on build {}, which no commit since {tag} has, so it \
                 could not be recounted once its key goes stale; {seed}",
                s.build
            ));
            continue;
        }
        let Some(&f) = against.get(name) else {
            errors.push(format!(
                "{gate}: {name}'s floor is stale and was not recounted"
            ));
            continue;
        };
        let t = sides.base_counts.get(name).copied();
        if sides.keys.get(name) == Some(&s.key) {
            let held = if s.build == sides.head_build {
                Some(("head", h))
            } else if s.build == tag_build {
                t.map(|t| ("tag", t))
            } else {
                None
            };
            if let Some((which, n)) = held
                && !rules.same(name, s.count, n)
            {
                errors.push(format!(
                    "{gate}: {name}'s floor {} is on the {which}'s build, which counts {n}; {seed}",
                    s.count
                ));
            }
        }
        if let Some(t) = t
            && f > t
            && !(rules.accepted)(name, ratio(f, t))
        {
            errors.push(format!(
                "{gate}: {name}'s floor {f} is above the tag's count {t}, and no accept line \
                 since the tag raises it so far"
            ));
        }
        if rules.raises(name, h, f) {
            errors.push(format!(
                "{gate}: an accept line raises {name}, so its floor is the head's {h}, not {f} \
                 (`cargo xtask {gate} --base {tag} --write-floors`)"
            ));
        }
    }
    Ok((against, errors))
}

/// At a tag, `--write-floors`: each counted name's floor is the tag's
/// count with the tag's build (a tie goes to the tag, the earlier build),
/// or the head's count with the head's build where the head counts less,
/// where an accept line raises it past the limit (as a landing does), or
/// where the tag has no count; names not counted keep their lines.
fn seed(root: &Path, gate: &str, sides: &Sides<'_>, rules: &Rules<'_>) -> Result<(), String> {
    let tag_build = build_id(root, Some(sides.base))?;
    let mut want = Floors::new();
    for (name, &h) in sides.head {
        let Some(key) = sides.keys.get(name) else {
            continue;
        };
        let (count, build) = match sides.base_counts.get(name) {
            Some(&t) if !rules.lowers(name, h, t) && !rules.raises(name, h, t) => {
                (t, tag_build.as_str())
            }
            _ => (h, sides.head_build),
        };
        want.insert(
            name.clone(),
            Floor {
                count,
                build: build.to_owned(),
                key: key.clone(),
            },
        );
    }
    let mut have = here(root, gate)?;
    write_merged(root, gate, &mut have, &want)?;
    println!("{gate}: seeded {} from the tag", file(gate));
    Ok(())
}

/// Writes `want` over `have`, keeping the names it does not hold.
fn write_merged(root: &Path, gate: &str, have: &mut Floors, want: &Floors) -> Result<(), String> {
    for (name, w) in want {
        have.insert(name.clone(), w.clone());
    }
    write(root, gate, have)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests fail on a broken fixture")]

    use super::*;
    use std::path::PathBuf;

    fn keyed(count: u64, build: &str, key: &str) -> Floor {
        Floor {
            count,
            build: build.to_owned(),
            key: key.to_owned(),
        }
    }

    fn floor(count: u64, build: &str) -> Floor {
        keyed(count, build, "k")
    }

    fn keys(names: &[&str], key: &str) -> BTreeMap<String, String> {
        names
            .iter()
            .map(|n| ((*n).to_owned(), key.to_owned()))
            .collect()
    }

    fn counts(of: &[(&str, u64)]) -> Counts {
        of.iter().map(|(n, c)| ((*n).to_owned(), *c)).collect()
    }

    /// A program's limit, native-ir's +0.5%.
    fn past(_: &str, h: u64, o: u64) -> bool {
        ratio(h, o) > 1.005
    }

    fn commit(dir: &Path, message: &str) -> String {
        let who = ["-c", "user.name=t", "-c", "user.email=t@t"];
        git(dir, &[&who[..], &["add", "--all"]].concat()).unwrap();
        git(
            dir,
            &[
                &who[..],
                &["commit", "--quiet", "--allow-empty", "-m", message],
            ]
            .concat(),
        )
        .unwrap();
        git(dir, &["rev-parse", "HEAD"]).unwrap()
    }

    /// A repository whose base commit holds `base` as native-ir's floors.
    fn repo(name: &str, base: &Floors) -> (PathBuf, String) {
        let dir = env::temp_dir().join(format!("floors-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "--quiet"]).unwrap();
        write(&dir, "native-ir", base).unwrap();
        let sha = commit(&dir, "base");
        (dir, sha)
    }

    #[test]
    fn a_floor_moves_only_to_the_head_and_rises_only_with_a_line_past_the_limit() {
        let base = Floors::from([
            ("fib".to_owned(), floor(1_000, "b-1")),
            ("dict".to_owned(), floor(2_000, "b-1")),
            ("loop".to_owned(), floor(3_000, "b-1")),
            ("str".to_owned(), floor(4_000, "b-1")),
        ]);
        let (dir, sha) = repo("moves", &base);
        let keys = keys(&["fib", "dict", "loop", "str", "aio"], "k");
        // fib gained; dict rose 0.6% under a `dict,str 1.007` line; str
        // rose 0.25%, under the limit, though the line names it; loop rose
        // 0.2% without a line; aio is new.
        let head = counts(&[
            ("fib", 990),
            ("dict", 2_012),
            ("loop", 3_006),
            ("str", 4_010),
            ("aio", 500),
        ]);
        let none = Counts::new();
        let sides = Sides {
            base: &sha,
            tag: false,
            head: &head,
            base_counts: &none,
            keys: &keys,
            head_build: "b-2",
            recounted: &none,
        };
        let accepted = |name: &str, r: f64| (name == "dict" || name == "str") && r <= 1.007;
        let rules = Rules {
            accepted: &accepted,
            past: &past,
            swept: &|_| false,
            band: exact,
        };
        // The file as the base has it: the raise and the new count are
        // missing; fib's gain passes and leaves its floor; str keeps its.
        let (against, errors) = judge(&dir, "native-ir", &sides, &rules, false).unwrap();
        assert_eq!(against["loop"], 3_000);
        let wrong: Vec<&String> = errors.iter().filter(|e| e.contains("must hold")).collect();
        assert_eq!(wrong.len(), 2, "{errors:?}");
        assert!(wrong.iter().any(|e| e.contains("dict\t2012\tb-2")));
        assert!(wrong.iter().any(|e| e.contains("aio\t500\tb-2")));
        // A new count may be seeded below the head, never above it.
        let mut seeded = base.clone();
        seeded.insert("aio".to_owned(), floor(480, "b-0"));
        write(&dir, "native-ir", &seeded).unwrap();
        let (against, errors) = judge(&dir, "native-ir", &sides, &rules, false).unwrap();
        assert!(!errors.iter().any(|e| e.contains("aio")), "{errors:?}");
        assert_eq!(against["aio"], 480, "the head is measured against the seed");
        seeded.insert("aio".to_owned(), floor(520, "b-0"));
        write(&dir, "native-ir", &seeded).unwrap();
        let (_, errors) = judge(&dir, "native-ir", &sides, &rules, false).unwrap();
        assert!(
            errors.iter().any(|e| e.contains("aio\t500\tb-2")),
            "{errors:?}"
        );
        // Written, it passes; loop and str keep the base's floors.
        let (_, errors) = judge(&dir, "native-ir", &sides, &rules, true).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let (_, errors) = judge(&dir, "native-ir", &sides, &rules, false).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let written = here(&dir, "native-ir").unwrap();
        assert_eq!(written["fib"], floor(990, "b-2"), "--write-floors lowers");
        assert_eq!(written["loop"], floor(3_000, "b-1"));
        assert_eq!(written["str"], floor(4_000, "b-1"));
        // A floor lowered below what the head counts, or raised without a
        // line, fails.
        let mut forged = written.clone();
        forged.insert("loop".to_owned(), floor(2_900, "b-2"));
        write(&dir, "native-ir", &forged).unwrap();
        let (_, errors) = judge(&dir, "native-ir", &sides, &rules, false).unwrap();
        assert!(
            errors.iter().any(|e| e.contains("loop\t3000\tb-1")),
            "{errors:?}"
        );
        let none_accepted = |_: &str, _: f64| false;
        let no_line = Rules {
            accepted: &none_accepted,
            past: &past,
            swept: &|_| false,
            band: exact,
        };
        let (_, errors) = judge(&dir, "native-ir", &sides, &no_line, true).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            here(&dir, "native-ir").unwrap()["dict"],
            floor(2_000, "b-1")
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Floors bind upward only: a drop passes with the file unchanged, a
    /// rise back to the floor passes, and a rise past it fails without an
    /// accept line and passes with one once written.
    #[test]
    fn a_drop_passes_and_only_a_rise_past_the_floor_needs_a_line() {
        let base = Floors::from([("fib".to_owned(), floor(1_000, "b-1"))]);
        let (dir, sha) = repo("upward", &base);
        let keys = keys(&["fib"], "k");
        let none = Counts::new();
        let never = |_: &str, _: f64| false;
        let line = |_: &str, r: f64| r <= 1.02;
        let judged = |h: u64, accepted: &dyn Fn(&str, f64) -> bool, write: bool| {
            let head = counts(&[("fib", h)]);
            let sides = Sides {
                base: &sha,
                tag: false,
                head: &head,
                base_counts: &none,
                keys: &keys,
                head_build: "b-2",
                recounted: &none,
            };
            let rules = Rules {
                accepted,
                past: &past,
                swept: &|_| false,
                band: exact,
            };
            judge(&dir, "native-ir", &sides, &rules, write).unwrap()
        };
        for h in [950, 1_000, 1_005] {
            let (against, errors) = judged(h, &never, false);
            assert!(errors.is_empty(), "{h}: {errors:?}");
            assert_eq!(against["fib"], 1_000);
            assert_eq!(here(&dir, "native-ir").unwrap(), base, "{h}");
        }
        // The floor is a bound: past it by more than the limit fails here
        // (the gate's own comparison fails it without a line, too).
        let (_, errors) = judged(1_010, &line, false);
        assert!(
            errors.iter().any(|e| e.contains("fib\t1010\tb-2")),
            "{errors:?}"
        );
        judged(1_010, &line, true);
        let (_, errors) = judged(1_010, &line, false);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(here(&dir, "native-ir").unwrap()["fib"], floor(1_010, "b-2"));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The review's WH mutant (wasm-ir): every program +0.45%, under the
    /// program limit, the geomean past its limit and accepted. The risen
    /// floors move to the head, so the next landing does not fail on the
    /// same geomean; with the geomean's rise not accepted, nothing moves.
    #[test]
    fn an_accepted_geomean_raises_the_floors_of_what_rose() {
        let names = ["a", "b", "c", "startup"];
        let base: Floors = names
            .iter()
            .map(|n| ((*n).to_owned(), floor(10_000, "b-1")))
            .collect();
        let (dir, sha) = repo("geomean", &base);
        let keys = keys(&names, "k");
        let head = counts(&[
            ("a", 10_045),
            ("b", 10_045),
            ("c", 10_000),
            ("startup", 10_001),
        ]);
        let none = Counts::new();
        let sides = Sides {
            base: &sha,
            tag: false,
            head: &head,
            base_counts: &none,
            keys: &keys,
            head_build: "b-2",
            recounted: &none,
        };
        let never = |_: &str, _: f64| false;
        let program = |name: &str| name != "startup";
        let floors_now: Counts = base.iter().map(|(n, f)| (n.clone(), f.count)).collect();
        assert!(geomean(&head, &floors_now, &none, &program) > 1.002);
        let swept = Rules {
            accepted: &never,
            past: &past,
            swept: &program,
            band: exact,
        };
        let (_, errors) = judge(&dir, "native-ir", &sides, &swept, false).unwrap();
        assert_eq!(
            errors.iter().filter(|e| e.contains("must hold")).count(),
            2,
            "{errors:?}"
        );
        judge(&dir, "native-ir", &sides, &swept, true).unwrap();
        let written = here(&dir, "native-ir").unwrap();
        assert_eq!(written["a"], floor(10_045, "b-2"));
        assert_eq!(written["b"], floor(10_045, "b-2"));
        assert_eq!(written["c"], floor(10_000, "b-1"), "c did not rise");
        assert_eq!(
            written["startup"],
            floor(10_000, "b-1"),
            "not in the geomean"
        );
        let not_swept = Rules {
            accepted: &never,
            past: &past,
            swept: &|_| false,
            band: exact,
        };
        write(&dir, "native-ir", &base).unwrap();
        let (_, errors) = judge(&dir, "native-ir", &sides, &not_swept, false).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A count that races (replay-ir's sessions): a drop passes either way,
    /// and `--write-floors` lowers the floor only past the band.
    #[test]
    fn a_count_within_its_band_keeps_the_floor() {
        let base = Floors::from([("s".to_owned(), floor(10_000, "b-1"))]);
        let (dir, sha) = repo("band", &base);
        let keys = keys(&["s"], "k");
        let none = Counts::new();
        let never = |_: &str, _: f64| false;
        let rules = Rules {
            accepted: &never,
            past: &past,
            swept: &|_| false,
            band: |_, f| f / 1_000,
        };
        for (h, moves) in [(9_990, false), (9_989, true)] {
            let head = counts(&[("s", h)]);
            let sides = Sides {
                base: &sha,
                tag: false,
                head: &head,
                base_counts: &none,
                keys: &keys,
                head_build: "b-2",
                recounted: &none,
            };
            write(&dir, "native-ir", &base).unwrap();
            let (against, errors) = judge(&dir, "native-ir", &sides, &rules, false).unwrap();
            assert_eq!(against["s"], 10_000);
            assert!(errors.is_empty(), "{h}: {errors:?}");
            judge(&dir, "native-ir", &sides, &rules, true).unwrap();
            let written = here(&dir, "native-ir").unwrap()["s"].count;
            assert_eq!(written, if moves { h } else { 10_000 }, "{h}");
        }
        fs::remove_dir_all(&dir).unwrap();
        // The same where the base has no floors (a gate's first landing):
        // the seed the head's file holds stands within the band.
        let (dir, sha) = repo("band-new", &Floors::new());
        write(&dir, "native-ir", &base).unwrap();
        for (h, moves) in [(9_990, false), (9_989, true)] {
            let head = counts(&[("s", h)]);
            let sides = Sides {
                base: &sha,
                tag: false,
                head: &head,
                base_counts: &none,
                keys: &keys,
                head_build: "b-2",
                recounted: &none,
            };
            let (_, errors) = judge(&dir, "native-ir", &sides, &rules, false).unwrap();
            assert_eq!(!errors.is_empty(), moves, "{h}: {errors:?}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Another host (another key) recounts the base's floors and judges
    /// against its recount; the file keeps the base's lines while no count
    /// moves, so two hosts both pass an unchanged tree, also after either
    /// ran `--write-floors` (the review's Ho0 to Ho5 and 8e1).
    #[test]
    fn two_hosts_both_pass_an_unchanged_tree() {
        let base = Floors::from([
            ("fib".to_owned(), keyed(1_000, "b-1", "k-a")),
            ("dict".to_owned(), keyed(2_000, "b-1", "k-a")),
        ]);
        let (dir, sha) = repo("hosts", &base);
        let none = Counts::new();
        let no_line = |_: &str, _: f64| false;
        let rules = Rules {
            accepted: &no_line,
            past: &past,
            swept: &|_| false,
            band: exact,
        };
        // Host b counts 4 more on fib than host a does, on every build.
        let (keys_a, keys_b) = (keys(&["fib", "dict"], "k-a"), keys(&["fib", "dict"], "k-b"));
        let head_a = counts(&[("fib", 1_000), ("dict", 2_000)]);
        let head_b = counts(&[("fib", 1_004), ("dict", 2_000)]);
        let side = |keys, head, recounted| Sides {
            base: &sha,
            tag: false,
            head,
            base_counts: &none,
            keys,
            head_build: "b-2",
            recounted,
        };
        for _ in 0..2 {
            let (_, errors) = judge(
                &dir,
                "native-ir",
                &side(&keys_a, &head_a, &none),
                &rules,
                false,
            )
            .unwrap();
            assert!(errors.is_empty(), "host a: {errors:?}");
            // Host b's recount of b-1 is its own count of the floors.
            let (against, errors) = judge(
                &dir,
                "native-ir",
                &side(&keys_b, &head_b, &head_b),
                &rules,
                false,
            )
            .unwrap();
            assert!(errors.is_empty(), "host b: {errors:?}");
            assert_eq!(against["fib"], 1_004);
            // --write-floors on either host leaves an unmoved file alone.
            judge(
                &dir,
                "native-ir",
                &side(&keys_b, &head_b, &head_b),
                &rules,
                true,
            )
            .unwrap();
            assert_eq!(here(&dir, "native-ir").unwrap(), base);
        }
        // A drop passes with the file unchanged; written, it takes this
        // host's line.
        let gain = counts(&[("fib", 994), ("dict", 2_000)]);
        let gained = side(&keys_b, &gain, &head_b);
        let (_, errors) = judge(&dir, "native-ir", &gained, &rules, false).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        judge(&dir, "native-ir", &gained, &rules, true).unwrap();
        assert_eq!(
            here(&dir, "native-ir").unwrap()["fib"],
            keyed(994, "b-2", "k-b")
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_recounted_floor_is_named_as_such() {
        // Two lines for one name would leave one unread.
        assert!(parse("native-ir", "fib\t1\tb-1\tk\nfib\t2\tb-1\tk\n").is_err());
        let base = Floors::from([("fib".to_owned(), keyed(1_000, "b-1", "k-old"))]);
        let (dir, sha) = repo("why", &base);
        write(
            &dir,
            "native-ir",
            &Floors::from([("fib".to_owned(), floor(1_000, "b-1"))]),
        )
        .unwrap();
        let none = Counts::new();
        let keys = keys(&["fib"], "k");
        let (head, recount) = (counts(&[("fib", 1_016)]), counts(&[("fib", 1_016)]));
        let sides = Sides {
            base: &sha,
            tag: false,
            head: &head,
            base_counts: &none,
            keys: &keys,
            head_build: "b-2",
            recounted: &recount,
        };
        let line = |_: &str, _: f64| true;
        let rules = Rules {
            accepted: &line,
            past: &past,
            swept: &|_| false,
            band: exact,
        };
        let (_, errors) = judge(&dir, "native-ir", &sides, &rules, false).unwrap();
        assert!(
            errors[0].contains("fib\t1000\tb-1\tk-old")
                && errors[0].contains("a stale floor is recounted"),
            "{errors:?}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The base a tag: the pre-tag check, and `--write-floors` seeding.
    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one scratch repository, case by case"
    )]
    fn at_a_tag_the_floors_are_the_tag_s_and_seeding_writes_them() {
        let (dir, sha) = repo("tag", &Floors::new());
        git(&dir, &["tag", "v0.0.9"]).unwrap();
        let tb = build_id(&dir, Some(&sha)).unwrap();
        let keys = keys(&["fib", "dict", "str", "loop", "aio"], "k");
        // fib gains on the tag; dict rose 0.6% under `dict 1.007`; str
        // and loop are the tag's; aio is new since the tag.
        let tag = counts(&[
            ("fib", 1_002),
            ("dict", 2_000),
            ("str", 700),
            ("loop", 3_000),
        ]);
        let head = counts(&[
            ("fib", 999),
            ("dict", 2_012),
            ("str", 700),
            ("loop", 3_000),
            ("aio", 500),
        ]);
        let none = Counts::new();
        let sides = Sides {
            base: &sha,
            tag: true,
            head: &head,
            base_counts: &tag,
            keys: &keys,
            head_build: "b-head",
            recounted: &none,
        };
        let accepted = |name: &str, r: f64| name == "dict" && r <= 1.007;
        let rules = Rules {
            accepted: &accepted,
            past: &past,
            swept: &|_| false,
            band: exact,
        };
        let good = Floors::from([
            ("fib".to_owned(), floor(999, "b-head")),
            ("dict".to_owned(), floor(2_012, "b-head")),
            ("str".to_owned(), floor(700, &tb)),
            ("loop".to_owned(), floor(3_000, &tb)),
            ("aio".to_owned(), floor(500, "b-head")),
        ]);
        let check = |file: &Floors| {
            write(&dir, "native-ir", file).unwrap();
            judge(&dir, "native-ir", &sides, &rules, false).unwrap().1
        };
        assert!(check(&good).is_empty(), "{:?}", check(&good));
        // Each wrong line fails, and the check writes nothing.
        let cases = [
            ("dict", floor(2_000, &tb), "an accept line raises dict"),
            (
                "str",
                floor(600, &tb),
                "on the tag's build, which counts 700",
            ),
            (
                "aio",
                floor(490, "b-head"),
                "on the head's build, which counts 500",
            ),
            ("loop", floor(3_000, "b-gone"), "just seed-floors v0.0.9"),
        ];
        for (name, line, says) in cases {
            let mut bad = good.clone();
            bad.insert(name.to_owned(), line.clone());
            let errors = check(&bad);
            assert!(
                errors.len() == 1 && errors[0].contains(says),
                "{name}: {errors:?}"
            );
            assert_eq!(here(&dir, "native-ir").unwrap()[name], line);
        }
        // A floor above the head's count passes at a tag too.
        let mut above = good.clone();
        above.insert("fib".to_owned(), floor(1_002, &tb));
        assert!(check(&above).is_empty(), "{:?}", check(&above));
        let mut gone = good.clone();
        gone.remove("str");
        assert!(check(&gone)[0].contains("no floor for str"));
        // Seeding: whatever the file held (another cycle's builds, a stale
        // key, a missing line), the counted lines become `good`; a line no
        // run counts stays.
        let mut old: Floors = good
            .keys()
            .map(|n| (n.clone(), keyed(1, "b-gone", "k-old")))
            .collect();
        old.remove("aio");
        old.insert("bogus".to_owned(), floor(1, "b-0"));
        write(&dir, "native-ir", &old).unwrap();
        assert!(
            stale(&dir, "native-ir", &sha, true, false, &keys)
                .unwrap_err()
                .contains("just seed-floors v0.0.9")
        );
        assert_eq!(
            stale(&dir, "native-ir", &sha, true, true, &keys).unwrap(),
            Vec::new()
        );
        let (_, errors) = judge(&dir, "native-ir", &sides, &rules, true).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let mut want = good.clone();
        want.insert("bogus".to_owned(), floor(1, "b-0"));
        assert_eq!(here(&dir, "native-ir").unwrap(), want);
        let text = fs::read_to_string(dir.join(file("native-ir"))).unwrap();
        judge(&dir, "native-ir", &sides, &rules, true).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join(file("native-ir"))).unwrap(),
            text
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A stale floor is recounted on its build wherever it is since the
    /// last tag: up to the base in a landing, up to HEAD at a tag.
    #[test]
    fn a_stale_floor_is_found_after_the_tag() {
        let (dir, tag) = repo("find", &Floors::new());
        git(&dir, &["tag", "v0.0.9"]).unwrap();
        fs::create_dir_all(dir.join("crates")).unwrap();
        fs::write(dir.join("crates/x.rs"), "fn x() {}\n").unwrap();
        let later = commit(&dir, "a build after the tag");
        let b = build_id(&dir, Some(&later)).unwrap();
        assert_ne!(b, build_id(&dir, Some(&tag)).unwrap());
        write(
            &dir,
            "native-ir",
            &Floors::from([("fib".to_owned(), keyed(9, &b, "k-old"))]),
        )
        .unwrap();
        commit(&dir, "floors");
        let keys = keys(&["fib"], "k");
        // Found in both modes, as a commit with that build (the newest).
        let head = git(&dir, &["rev-parse", "HEAD"]).unwrap();
        for (base, tag_base) in [(&tag, true), (&head, false)] {
            let found = stale(&dir, "native-ir", base, tag_base, false, &keys).unwrap();
            assert_eq!(found.len(), 1, "{found:?}");
            assert_eq!(found[0].1, vec!["fib".to_owned()]);
            assert_eq!(build_id(&dir, Some(&found[0].0)).unwrap(), b);
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
