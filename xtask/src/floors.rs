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
//! Against the base's file ([`judge`]), the head's file must be what the
//! head's counts make of it:
//! - a count below its floor lowers the floor to it, in the same change:
//!   an unrecorded gain must not become headroom;
//! - a floor rises only to the head's count, and only with an accept line
//!   ([`crate::accept`]) that names the count with a ceiling at or above
//!   the rise over the floor;
//! - any other floor stays as the base has it.
//!
//! `--write-floors` writes that file. Against a `v*` tag as the base
//! (`just pretag`), the head's floors are checked against the tag's counts
//! and the head's instead: none above the tag's count without a line since
//! the tag, none above the head's. There `--write-floors` seeds the file
//! from the tag ([`seed`]): each floor is the tag's count with the tag's
//! build, or the head's where the head counts less, whatever the file held
//! before, so a release starts its floors at its own counts (`just
//! seed-floors`, the first commit after a tag).
//!
//! Two landings that both change a floor conflict on its line. Take the
//! raise when one side is an accepted raise, else the lower count; the
//! gate on the combined tree then sets it to the combined count.

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
        out.insert(
            name.to_owned(),
            Floor {
                count,
                build: build.to_owned(),
                key: key.to_owned(),
            },
        );
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

/// The commit whose build is `build`, newest first on the first-parent
/// line from `base` back to the last `v*` tag it descends from.
pub(crate) fn find_build(root: &Path, base: &str, build: &str) -> Result<Option<String>, String> {
    let tag = git(
        root,
        &["describe", "--tags", "--abbrev=0", "--match", "v*", base],
    )
    .ok();
    let range = tag
        .as_ref()
        .map_or_else(|| base.to_owned(), |t| format!("{t}^..{base}"));
    for sha in git(root, &["rev-list", "--first-parent", &range])?.lines() {
        if build_id(root, Some(sha))? == build {
            return Ok(Some(sha.to_owned()));
        }
    }
    Ok(None)
}

/// The floors `gate` starts from (the base's, or at a tag the working
/// tree's) whose key is not `keys`' (stale): the commit of each one's
/// build, which the gate recounts, and the names it recounts. None when
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
    let mut builds: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (name, f) in &floors {
        if keys.get(name).is_some_and(|k| *k != f.key) {
            builds.entry(&f.build).or_default().push(name.clone());
        }
    }
    let mut out = Vec::new();
    for (build, names) in builds {
        let Some(sha) = find_build(root, base, build)? else {
            return Err(format!(
                "{gate}: the floors of {} are stale (another counting method, tool, host or \
                 program source) and no commit since the last tag has their build {build}",
                names.join(", ")
            ));
        };
        println!(
            "{gate}: the floors of {} are stale: recounting their build, {}",
            names.join(", "),
            &sha[..sha.len().min(10)]
        );
        out.push((sha, names));
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

/// The floors the head is measured against, and what is wrong with the
/// head's floor file (empty when nothing). `raised(name, ratio)`: an
/// accept line since the base names `name` with a ceiling at or above
/// `ratio`. With `write`, writes the file the head must have first (at a
/// tag, the file [`seed`] makes).
#[allow(clippy::too_many_lines, reason = "the pre-tag check, then a landing's")]
pub(crate) fn judge(
    root: &Path,
    gate: &str,
    sides: &Sides<'_>,
    raised: &dyn Fn(&str, f64) -> bool,
    write: bool,
) -> Result<(Counts, Vec<String>), String> {
    if sides.tag && write {
        seed(root, gate, sides)?;
    }
    let source = if sides.tag {
        here(root, gate)?
    } else {
        at(root, sides.base, gate)?
    };
    // The floors as this run counts them.
    let mut floors = source.clone();
    for (name, f) in &mut floors {
        if let (Some(&n), Some(k)) = (sides.recounted.get(name), sides.keys.get(name)) {
            f.count = n;
            f.key.clone_from(k);
        }
    }
    let against: Counts = floors
        .iter()
        .filter(|(name, f)| sides.keys.get(*name) == Some(&f.key))
        .map(|(name, f)| (name.clone(), f.count))
        .collect();
    let mut errors = Vec::new();
    let ratio = |a: u64, b: u64| {
        #[allow(clippy::cast_precision_loss, reason = "a ratio of counts")]
        let r = a as f64 / b.max(1) as f64;
        r
    };
    if sides.tag {
        // The pre-tag check: the head's floors against the tag's counts
        // and the head's.
        for (name, &h) in sides.head {
            let Some(&f) = against.get(name) else {
                errors.push(format!(
                    "{gate}: no floor for {name} (or a stale one) in {}",
                    file(gate)
                ));
                continue;
            };
            let t = sides.base_counts.get(name).copied().unwrap_or(f);
            if f > t && !raised(name, ratio(f, t)) {
                errors.push(format!(
                    "{gate}: {name}'s floor {f} is above the tag's count {t}, and no accept \
                     line since the tag raises it so far"
                ));
            }
            if h < f {
                errors.push(format!(
                    "{gate}: {name} is {h}, below its floor {f}: lower the floor \
                     (`cargo xtask {gate} --write-floors`)"
                ));
            }
        }
        return Ok((against, errors));
    }
    // A landing: the file the head must have. A stale floor the gate
    // could not recount is not a floor to measure or replace.
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
    let have = here(root, gate)?;
    let mut want = floors.clone();
    for (name, &h) in sides.head {
        let Some(key) = sides.keys.get(name) else {
            continue;
        };
        let at_head = Floor {
            count: h,
            build: sides.head_build.to_owned(),
            key: key.clone(),
        };
        match floors.get(name) {
            // A count the base has no floor for: the head's file may seed
            // it at or below the head's count (the least since the tag,
            // which the first landing of the floors takes), never above.
            None => {
                let seed = have
                    .get(name)
                    .filter(|f| f.count <= h && f.key == *key)
                    .cloned();
                want.insert(name.clone(), seed.unwrap_or(at_head));
            }
            Some(f) if h < f.count || (h > f.count && raised(name, ratio(h, f.count))) => {
                want.insert(name.clone(), at_head);
            }
            Some(_) => {}
        }
    }
    // The head is measured against the base's floors, and a new count
    // against its seed.
    let mut against = against;
    for (name, w) in &want {
        if !floors.contains_key(name) && sides.keys.get(name) == Some(&w.key) {
            against.insert(name.clone(), w.count);
        }
    }
    let mut have = have;
    if write && have != want {
        write_merged(root, gate, &mut have, &want)?;
        println!("{gate}: wrote {}", file(gate));
    }
    for (name, w) in &want {
        let h = have.get(name);
        if h == Some(w) {
            continue;
        }
        let was = source.get(name).map(|f| f.count);
        let why = match (was, sides.head.get(name)) {
            (Some(b), Some(&n)) if n < b => format!("the head's {n} is below the base's floor {b}"),
            (Some(b), Some(&n)) if n > b && w.count == n => {
                format!("an accept line raises it from {b} to the head's {n}")
            }
            (Some(_), _) if sides.recounted.contains_key(name) => {
                "the base's floor was stale and is recounted".to_owned()
            }
            (None, _) => "a new count".to_owned(),
            _ => "only the head's count or an accept line moves a floor".to_owned(),
        };
        errors.push(format!(
            "{gate}: {} must hold `{name}\t{}\t{}\t{}` ({why}){}",
            file(gate),
            w.count,
            w.build,
            w.key,
            h.map_or_else(String::new, |h| format!(", not {}", h.count))
        ));
    }
    if !errors.is_empty() {
        errors.push(format!(
            "{gate}: `cargo xtask {gate} --write-floors` writes it; commit it with the change"
        ));
    }
    Ok((against, errors))
}

/// At a tag, `--write-floors`: each counted name's floor is the tag's
/// count with the tag's build (a tie goes to the tag, the earlier build),
/// or the head's count with the head's build where the head counts less
/// or the tag has no count; names not counted this run keep their lines.
fn seed(root: &Path, gate: &str, sides: &Sides<'_>) -> Result<(), String> {
    let tag_build = build_id(root, Some(sides.base))?;
    let mut want = Floors::new();
    for (name, &h) in sides.head {
        let Some(key) = sides.keys.get(name) else {
            continue;
        };
        let (count, build) = match sides.base_counts.get(name) {
            Some(&t) if t <= h => (t, tag_build.as_str()),
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

    fn floor(count: u64, build: &str) -> Floor {
        Floor {
            count,
            build: build.to_owned(),
            key: "k".to_owned(),
        }
    }

    /// A repository whose base commit holds `base` as native-ir's floors.
    fn repo(name: &str, base: &Floors) -> (PathBuf, String) {
        let dir = env::temp_dir().join(format!("floors-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "--quiet"]).unwrap();
        write(&dir, "native-ir", base).unwrap();
        let who = ["-c", "user.name=t", "-c", "user.email=t@t"];
        git(&dir, &[&who[..], &["add", "--all"]].concat()).unwrap();
        git(
            &dir,
            &[&who[..], &["commit", "--quiet", "-m", "base"]].concat(),
        )
        .unwrap();
        let sha = git(&dir, &["rev-parse", "HEAD"]).unwrap();
        (dir, sha)
    }

    #[test]
    fn a_floor_moves_only_to_the_head_and_rises_only_with_a_line() {
        let base = Floors::from([
            ("fib".to_owned(), floor(1_000, "b-1")),
            ("dict".to_owned(), floor(2_000, "b-1")),
            ("loop".to_owned(), floor(3_000, "b-1")),
        ]);
        let (dir, sha) = repo("moves", &base);
        let keys: BTreeMap<String, String> = ["fib", "dict", "loop", "aio"]
            .into_iter()
            .map(|n| (n.to_owned(), "k".to_owned()))
            .collect();
        // fib gained, dict rose 0.4% under a `dict 1.005` line, loop rose
        // 0.2% without one, aio is new.
        let head = Counts::from([
            ("fib".to_owned(), 990),
            ("dict".to_owned(), 2_008),
            ("loop".to_owned(), 3_006),
            ("aio".to_owned(), 500),
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
        let raised = |name: &str, r: f64| name == "dict" && r <= 1.005;
        // The file as the base has it: the gain, the raise and the new
        // count are each missing.
        let (against, errors) = judge(&dir, "native-ir", &sides, &raised, false).unwrap();
        assert_eq!(against["loop"], 3_000);
        let wrong: Vec<&String> = errors.iter().filter(|e| e.contains("must hold")).collect();
        assert_eq!(wrong.len(), 3, "{errors:?}");
        assert!(wrong.iter().any(|e| e.contains("fib\t990\tb-2")));
        assert!(wrong.iter().any(|e| e.contains("dict\t2008\tb-2")));
        assert!(wrong.iter().any(|e| e.contains("aio\t500\tb-2")));
        // A new count may be seeded below the head (the least since the
        // tag), never above it.
        let mut seeded = base.clone();
        seeded.insert("aio".to_owned(), floor(480, "b-0"));
        write(&dir, "native-ir", &seeded).unwrap();
        let (against, errors) = judge(&dir, "native-ir", &sides, &raised, false).unwrap();
        assert!(!errors.iter().any(|e| e.contains("aio")), "{errors:?}");
        assert_eq!(against["aio"], 480, "the head is measured against the seed");
        seeded.insert("aio".to_owned(), floor(520, "b-0"));
        write(&dir, "native-ir", &seeded).unwrap();
        let (_, errors) = judge(&dir, "native-ir", &sides, &raised, false).unwrap();
        assert!(
            errors.iter().any(|e| e.contains("aio\t500\tb-2")),
            "{errors:?}"
        );
        // Written, it passes, and loop keeps the base's floor.
        let (_, errors) = judge(&dir, "native-ir", &sides, &raised, true).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            here(&dir, "native-ir").unwrap()["loop"],
            floor(3_000, "b-1")
        );
        // A floor lowered below what the head counts, or raised without a
        // line, fails.
        let mut forged = here(&dir, "native-ir").unwrap();
        forged.insert("loop".to_owned(), floor(2_900, "b-2"));
        write(&dir, "native-ir", &forged).unwrap();
        let (_, errors) = judge(&dir, "native-ir", &sides, &raised, false).unwrap();
        assert!(
            errors.iter().any(|e| e.contains("loop\t3000\tb-1")),
            "{errors:?}"
        );
        let no_line = |_: &str, _: f64| false;
        let (_, errors) = judge(&dir, "native-ir", &sides, &no_line, true).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            here(&dir, "native-ir").unwrap()["dict"],
            floor(2_000, "b-1")
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_pre_tag_check_holds_floors_to_the_tag_and_the_head() {
        let (dir, sha) = repo("tag", &Floors::new());
        let floors = Floors::from([
            ("fib".to_owned(), floor(1_000, "b-1")),
            ("dict".to_owned(), floor(2_010, "b-1")),
        ]);
        write(&dir, "native-ir", &floors).unwrap();
        let keys: BTreeMap<String, String> = ["fib", "dict"]
            .into_iter()
            .map(|n| (n.to_owned(), "k".to_owned()))
            .collect();
        let tag = Counts::from([("fib".to_owned(), 1_002), ("dict".to_owned(), 2_000)]);
        let head = Counts::from([("fib".to_owned(), 999), ("dict".to_owned(), 2_010)]);
        let none = Counts::new();
        let sides = Sides {
            base: &sha,
            tag: true,
            head: &head,
            base_counts: &tag,
            keys: &keys,
            head_build: "b-2",
            recounted: &none,
        };
        // dict's floor is above the tag's count; fib's head is below its
        // floor.
        let (_, errors) = judge(&dir, "native-ir", &sides, &|_, _| false, false).unwrap();
        assert_eq!(errors.len(), 2, "{errors:?}");
        let line = |name: &str, r: f64| name == "dict" && r <= 1.006;
        let (_, errors) = judge(&dir, "native-ir", &sides, &line, false).unwrap();
        assert!(
            errors.len() == 1 && errors[0].contains("fib is 999"),
            "{errors:?}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn at_a_tag_write_floors_seeds_the_tag_s_counts_whatever_the_file_held() {
        let (dir, sha) = repo("seed", &Floors::new());
        // The last cycle's floors: fib's build is in no commit here and its
        // key is another method's, so only seeding can replace it; loop is
        // not counted this run.
        let old = Floor {
            count: 900,
            build: "b-gone".to_owned(),
            key: "k-old".to_owned(),
        };
        let mut before = Floors::from([
            ("fib".to_owned(), old),
            ("dict".to_owned(), floor(1_990, "b-1")),
            ("loop".to_owned(), floor(3_000, "b-1")),
        ]);
        write(&dir, "native-ir", &before).unwrap();
        let keys: BTreeMap<String, String> = ["fib", "dict", "str", "aio"]
            .into_iter()
            .map(|n| (n.to_owned(), "k".to_owned()))
            .collect();
        assert!(stale(&dir, "native-ir", &sha, true, false, &keys).is_err());
        let none_stale: Vec<(String, Vec<String>)> = Vec::new();
        assert_eq!(
            stale(&dir, "native-ir", &sha, true, true, &keys).unwrap(),
            none_stale
        );
        // The head counts less than the tag on fib, more on dict, the same
        // on str; aio is new since the tag.
        let tag = Counts::from([
            ("fib".to_owned(), 1_002),
            ("dict".to_owned(), 2_000),
            ("str".to_owned(), 700),
        ]);
        let head = Counts::from([
            ("fib".to_owned(), 999),
            ("dict".to_owned(), 2_010),
            ("str".to_owned(), 700),
            ("aio".to_owned(), 500),
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
        let (against, errors) = judge(&dir, "native-ir", &sides, &|_, _| false, true).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let tag_build = build_id(&dir, Some(&sha)).unwrap();
        let seeded = here(&dir, "native-ir").unwrap();
        assert_eq!(seeded["fib"], floor(999, "b-head"));
        assert_eq!(seeded["dict"], floor(2_000, &tag_build));
        assert_eq!(seeded["str"], floor(700, &tag_build));
        assert_eq!(seeded["aio"], floor(500, "b-head"));
        assert_eq!(seeded["loop"], floor(3_000, "b-1"));
        assert_eq!(against["dict"], 2_000);
        // Seeding again changes nothing, and the pre-tag check passes it.
        let text = fs::read_to_string(dir.join(file("native-ir"))).unwrap();
        let (_, errors) = judge(&dir, "native-ir", &sides, &|_, _| false, true).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            fs::read_to_string(dir.join(file("native-ir"))).unwrap(),
            text
        );
        let (_, errors) = judge(&dir, "native-ir", &sides, &|_, _| false, false).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        // Without `write` the old file is only checked: dict's 1,990 is
        // below the head's 2,010 and stays.
        before.remove("fib");
        write(&dir, "native-ir", &before).unwrap();
        let (_, errors) = judge(&dir, "native-ir", &sides, &|_, _| false, false).unwrap();
        assert!(errors.iter().any(|e| e.contains("fib")), "{errors:?}");
        assert_eq!(
            here(&dir, "native-ir").unwrap()["dict"],
            floor(1_990, "b-1")
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
