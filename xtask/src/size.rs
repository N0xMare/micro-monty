//! `cargo xtask size`'s gate: each shipped build against the base's and
//! the release's, all three got on this host (docs/spec.md §6).
//!
//! The base is the merge base of `HEAD` and `--base REV` (default
//! `origin/main`); the release is the latest `v*` tag the base descends
//! from. Each is checked out in a worktree outside the checkout. A
//! revision whose xtask has `size --head` ([`crate::SIZE_HEAD`]) builds
//! itself there with its own recipe, so a change to the recipe (the
//! `wasm-opt` flags, `RUSTFLAGS`, cargo arguments) is charged like any
//! other rise; an older one (v0.0.1) is got with the head's recipe
//! ([`crate::shipped_sizes`]). A commit's sizes are cached in
//! `target/size/cache/`, keyed by the recipe that built them
//! ([`recipe_key`]: the toolchain, `wasm-opt` and the xtask code that
//! picks the flags), so a revision is got once per machine and recipe. A clean committed head caches its
//! own sizes, so after it lands it is the next change's base for free.
//! Sizes are also cached under a hash of the build's inputs ([`inputs`]),
//! so a base that changed nothing a shipped build reads (docs, tests,
//! conformance results) reuses the sizes of one that had them; the
//! recipe is in the key, not in the inputs.
//!
//! - Any growth past the base, one byte or more, needs git trailers
//!   `Size: <artifact> <±bytes> <why>` in the commits since the base, each
//!   giving its own commit's change, whose `<bytes>` summed cover the
//!   rise and exceed the change by at most [`slack`]. A commit that a
//!   later one reverts counts for nothing. Growth in a tree with
//!   uncommitted changes, untracked build inputs included, always fails:
//!   no commit message can cover it.
//! - Growth past the release needs a budget: the last line
//!   `<tag> <artifact> <±bytes> <why>` for that release and artifact in
//!   [`BUDGET`] as the base holds it, 0 without one. The owner raises it by
//!   appending a line.
//! - The head's artifact names must be the base's and the release's, and a
//!   checkout without a `v*` tag fails, naming the fetch.
//!
//! The reference trees and their builds live outside the checkout
//! ([`crate::shipped::refs_dir`]), so none reads its `.cargo/`. A shrink
//! needs nothing. `xtask/size-ceilings.txt`, the ceilings until
//! 2026-11-13, is history.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    wasm_wall::{checkout, git},
    workspace_root,
};

/// Each release cycle's budgets: `<tag> <artifact> <bytes> <why>` lines,
/// appended, never edited; the last line for a tag and artifact holds.
const BUDGET: &str = "xtask/size-budget.txt";

/// What a cached size means: change it when the recipe changes.
const RECIPE: &str = "shipped sizes 1";

pub(crate) type Sizes = BTreeMap<String, u64>;

/// Builds a tree's shipped artifacts into a target directory and returns
/// their sizes ([`crate::shipped_sizes`]).
type Build<'a> = &'a dyn Fn(&Path, &Path) -> Result<Vec<(&'static str, u64)>, String>;

/// Checks the head's sizes `measured` against the base's and the
/// release's, building them with `build` when they are not cached. With
/// no `measured`, only gets the base's and the release's into the cache
/// (`check` does that beside the head's build).
pub(crate) fn gate(
    measured: Option<&Sizes>,
    base_rev: &str,
    build: Build<'_>,
) -> Result<(), String> {
    let root = workspace_root();
    // The cache stays in this checkout's target/; the reference trees and
    // their builds are outside it ([`crate::shipped::refs_dir`]).
    let cache = root.join("target/size/cache");
    let refs = crate::shipped::refs_dir("size");
    let key = recipe_key(&root, &root)?;
    let committed = uncommitted(&root)?.is_empty();
    let base = git(&root, &["merge-base", base_rev, "HEAD"])
        .map_err(|e| format!("size: no merge base with {base_rev} to measure against: {e}"))?;
    let tag = git(
        &root,
        &["describe", "--tags", "--abbrev=0", "--match", "v*", &base],
    )
    .map_err(|_| no_tag(&root, &base))?;
    let revisions = vec![
        ("base", base.clone(), base.clone()),
        (
            "release",
            tag.clone(),
            git(&root, &["rev-parse", &format!("{tag}^{{commit}}")])?,
        ),
    ];
    let mut sized = Vec::new();
    for (label, name, sha) in revisions {
        let tree = refs.join(format!("{label}-src"));
        let own = builds_itself(&root, &sha);
        // Checked out either way: a tree built with the head's recipe still
        // keeps its own toolchain, which is in the key.
        checkout(&root, &tree, &sha)?;
        let key = if own {
            println!("size: the {label}, {name}, with its own recipe");
            recipe_key(&tree, &tree)?
        } else {
            println!(
                "size: the {label}, {name}, with the head's recipe and its own toolchain (its \
                 xtask has no `size --head`)"
            );
            recipe_key(&root, &tree)?
        };
        let same = inputs(&root, &sha)?;
        let cached = load(&cache, &sha, &key).or_else(|| load(&cache, &same, &key));
        let got = if let Some(cached) = cached {
            save(&cache, &sha, &key, &cached)?;
            cached
        } else {
            println!("size: building the {label}, {name} (cached afterwards)");
            let got: Sizes = if own {
                build_itself(&tree)?
            } else {
                build(&tree, &refs.join(label))?
                    .into_iter()
                    .map(|(a, b)| (a.to_owned(), b))
                    .collect()
            };
            save(&cache, &sha, &key, &got)?;
            save(&cache, &same, &key, &got)?;
            got
        };
        sized.push(got);
    }
    let Some(measured) = measured else {
        return Ok(());
    };
    let head = measured.clone();
    if committed {
        save(&cache, &git(&root, &["rev-parse", "HEAD"])?, &key, &head)?;
        save(&cache, &inputs(&root, "HEAD")?, &key, &head)?;
    }
    // Uncommitted changes: no commit message speaks for the tree.
    let trailers = if committed {
        trailers(&git(&root, &["log", LOG_FORMAT, &format!("{base}..HEAD")])?)
    } else {
        println!(
            "size: uncommitted changes ({}), so no `Size:` trailer counts",
            uncommitted(&root)?
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join(", ")
        );
        Vec::new()
    };
    // The budgets the base holds: a change cannot raise its own.
    let budget = git(&root, &["show", &format!("{base}:{BUDGET}")]).unwrap_or_default();
    let budgets = budgets(&budget)?;
    let failed = judge(
        &head,
        &sized[0],
        Some((tag.as_str(), &sized[1])),
        &budgets,
        &trailers,
    );
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("\n"))
    }
}

/// `git log`'s format for [`trailers`]: per commit its id, its message
/// and its `Size:` trailers as git parses them (the trailer block at the
/// end of the message, so a quoted or indented `Size:` line is none).
const LOG_FORMAT: &str =
    "--format=%H%x1f%B%x1f%(trailers:key=Size,valueonly,unfold,separator=%x1e)%x1d";

/// The `Size: <artifact> <±bytes> <why>` trailers of the commits in a
/// [`LOG_FORMAT`] log (newest first), as (artifact, bytes). A commit that
/// a later counted commit reverts (`This reverts commit <id>.`) counts for
/// nothing: the revert undid its rise, so its trailer covers no other.
/// A trailer that is not `<artifact> <±bytes> <why>` is reported as an
/// artifact named after the whole line, which no artifact matches.
fn trailers(log: &str) -> Vec<(String, i128)> {
    let mut dropped = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for record in log.split('\x1d').map(str::trim).filter(|r| !r.is_empty()) {
        let mut fields = record.split('\x1f');
        let (sha, body, values) = (
            fields.next().unwrap_or_default().trim(),
            fields.next().unwrap_or_default(),
            fields.next().unwrap_or_default(),
        );
        if dropped.contains(sha) {
            continue;
        }
        if let Some(rest) = body.split("This reverts commit ").nth(1) {
            let reverted: String = rest.chars().take_while(char::is_ascii_hexdigit).collect();
            dropped.insert(reverted);
        }
        for value in values
            .split('\x1e')
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            let mut f = value.split_whitespace();
            let entry = match (f.next(), f.next(), f.next()) {
                (Some(a), Some(b), Some(_why)) if b.starts_with(['+', '-']) => {
                    b.parse::<i128>().ok().map(|n| (a.to_owned(), n))
                }
                _ => None,
            };
            out.push(entry.unwrap_or_else(|| (format!("`Size: {value}`"), 0)));
        }
    }
    out
}

/// How far the trailers may overstate a rise: a rebase moves sizes a
/// little, but `Size: hello.wasm +999999` must not cover anything.
fn slack(covered: i128) -> i128 {
    (covered / 10).max(1024)
}

/// Why the head's sizes fail against the `base`'s, the `release`'s (its
/// tag and sizes) and that release's `budgets`, given the `trailers`
/// since the base ([`trailers`]).
fn judge(
    head: &Sizes,
    base: &Sizes,
    release: Option<(&str, &Sizes)>,
    budgets: &BTreeMap<(String, String), i64>,
    trailers: &[(String, i128)],
) -> Vec<String> {
    let mut failed = Vec::new();
    // A renamed or dropped artifact would leave its rise unmeasured.
    for (against, sizes) in [Some(("the base", base)), release].into_iter().flatten() {
        if head.keys().ne(sizes.keys()) {
            failed.push(format!(
                "the head's artifacts ({}) are not {against}'s ({})",
                head.keys().cloned().collect::<Vec<_>>().join(", "),
                sizes.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    for (artifact, _) in trailers {
        if !head.contains_key(artifact) {
            failed.push(format!(
                "a `Size:` trailer names {artifact}, which is no shipped artifact; the form is \
                 `Size: <artifact> <±bytes> <why>`"
            ));
        }
    }
    // The trailers since the base, summed: each records its own commit's
    // change, and the measured rise is the net of every commit since the
    // base, so a commit that shrinks the artifact without a trailer leaves
    // part of the sum unused, within the slack.
    let covered = |artifact: &str| -> i128 {
        trailers
            .iter()
            .filter(|(a, _)| a == artifact)
            .map(|(_, n)| n)
            .sum()
    };
    for (artifact, &bytes) in head {
        let mut line = format!("size: {artifact} {bytes} bytes");
        if let Some(&old) = base.get(artifact) {
            let delta = i128::from(bytes) - i128::from(old);
            let covered = covered(artifact);
            let _ = write!(line, ", base {old} ({delta:+})");
            if covered != 0 {
                let _ = write!(line, ", trailers {covered:+}");
            }
            if delta > covered {
                failed.push(format!(
                    "{artifact} grew {delta:+} bytes past the base, {covered:+} of it in `Size:` \
                     trailers: commit it with the trailer `Size: {artifact} {:+} <why>`",
                    delta - covered
                ));
            } else if covered - delta.max(0) > slack(covered) {
                failed.push(format!(
                    "the `Size:` trailers for {artifact} add up to {covered:+} bytes, but it \
                     changed by {delta:+}: a trailer gives its commit's own change (a shrink may \
                     carry a negative one), within {} bytes",
                    slack(covered)
                ));
            }
        }
        if let Some((tag, sizes)) = release
            && let Some(&old) = sizes.get(artifact)
        {
            let growth = i128::from(bytes) - i128::from(old);
            let allowed = budgets
                .get(&(tag.to_owned(), artifact.clone()))
                .copied()
                .unwrap_or(0);
            let _ = write!(
                line,
                ", {tag} {old} ({growth:+} of a {allowed:+}-byte budget)"
            );
            if growth > i128::from(allowed) {
                failed.push(format!(
                    "{artifact} is {growth:+} bytes past {tag}, over its {allowed:+}-byte budget: \
                     only the owner raises it, with a line `{tag} {artifact} <±bytes> <decision>` \
                     in {BUDGET}"
                ));
            }
        }
        println!("{line}");
    }
    failed
}

/// The paths a change to which the gate cannot see in a commit: tracked
/// files with uncommitted changes, and untracked files under the build's
/// [`INPUTS`] and the recipe (xtask/), as `git status --porcelain` lines.
fn uncommitted(root: &Path) -> Result<String, String> {
    let mut lines = git(root, &["status", "--porcelain", "--untracked-files=no"])?;
    let mut args = vec!["ls-files", "--others", "--exclude-standard", "--"];
    args.extend(INPUTS);
    args.push("xtask");
    for path in git(root, &args)?.lines() {
        let _ = write!(lines, "\n?? {path}");
    }
    Ok(lines.trim().to_owned())
}

/// The error when no `v*` tag is reachable from `base`: a shallow or
/// tagless clone has no release to measure against.
fn no_tag(root: &Path, base: &str) -> String {
    let shallow = git(root, &["rev-parse", "--is-shallow-repository"]).unwrap_or_default();
    let fetch = if shallow.trim() == "true" {
        "git fetch --tags --unshallow origin"
    } else {
        "git fetch --tags origin"
    };
    format!(
        "size: no v* tag is reachable from the base {}, so there is no release to measure \
         against: run `{fetch}`",
        &base[..base.len().min(10)]
    )
}

/// The budgets in `text`: the last `<tag> <artifact> <bytes> <why>` line
/// per tag and artifact.
fn budgets(text: &str) -> Result<BTreeMap<(String, String), i64>, String> {
    let mut out = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(tag), Some(artifact), Some(bytes), Some(_why)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(format!(
                "{BUDGET}:{}: not `<tag> <artifact> <bytes> <why>`",
                n + 1
            ));
        };
        let bytes = bytes.parse().map_err(|_| {
            format!(
                "{BUDGET}:{}: `{bytes}` is not a number of bytes (a sign is allowed)",
                n + 1
            )
        })?;
        out.insert((tag.to_owned(), artifact.to_owned()), bytes);
    }
    Ok(out)
}

/// The xtask files that hold the shipped recipe: `SHIPPED_RUSTFLAGS`, the
/// cargo arguments of each artifact, the `wasm-opt` arguments and the
/// bundle guest's sections. Code that moves part of the recipe to another
/// file adds that file here. A tree from before shipped.rs (v0.0.1 to the
/// commit that added it) kept the recipe in main.rs: [`OLD_RECIPE_FILES`].
const RECIPE_FILES: [&str; 2] = ["xtask/src/shipped.rs", "xtask/src/wasm.rs"];
const OLD_RECIPE_FILES: [&str; 2] = ["xtask/src/main.rs", "xtask/src/wasm.rs"];

/// The cache key of sizes built with the recipe of the tree at `tree` and
/// the toolchain of the tree at `toolchain` (another revision built with
/// this checkout's recipe keeps its own rust-toolchain.toml): [`RECIPE`],
/// `rustc -vV` in `toolchain`, `wasm-opt`'s version and a hash of
/// `tree`'s [`RECIPE_FILES`] as they are in its working tree. A size
/// cached under another recipe is never compared with one built under
/// this one, and a head that lands caches its sizes under the key it gets
/// as the next change's base.
pub(crate) fn recipe_key(tree: &Path, toolchain: &Path) -> Result<String, String> {
    let files = if tree.join(RECIPE_FILES[0]).exists() {
        RECIPE_FILES
    } else {
        OLD_RECIPE_FILES
    };
    let mut code = Vec::new();
    for file in files {
        let text = fs::read(tree.join(file)).map_err(|e| format!("cannot read {file}: {e}"))?;
        code.extend_from_slice(&text);
    }
    let code = crate::wasm_ir::source_hash(&code);
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let output = Command::new(&rustc)
        .arg("-vV")
        .current_dir(toolchain)
        .env_remove("RUSTUP_TOOLCHAIN")
        .output()
        .map_err(|e| format!("cannot run {rustc}: {e}"))?;
    if !output.status.success() {
        return Err(format!("{rustc} -vV failed in {}", tree.display()));
    }
    let rustc = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let wasm_opt = crate::wasm::tool("WASM_OPT", "wasm-opt").ok_or(
        "wasm-opt (binaryen) not found ($WASM_OPT, ~/.local/bin, PATH); run .agents/setup",
    )?;
    let wasm_opt = crate::command_output(&wasm_opt, &["--version"])?;
    Ok(format!(
        "{RECIPE}; xtask {code}; {}; {wasm_opt}",
        rustc.replace('\n', "; ")
    ))
}

/// Whether commit `sha`'s xtask builds a tree's shipped sizes with its own
/// recipe (`size --head`): its main.rs carries [`crate::SIZE_HEAD`].
pub(crate) fn builds_itself(root: &Path, sha: &str) -> bool {
    let args = [
        "grep",
        "-q",
        "-F",
        "-e",
        crate::SIZE_HEAD,
        sha,
        "--",
        "xtask/src/main.rs",
    ];
    git(root, &args).is_ok()
}

/// The tree at `tree` builds its shipped artifacts with its own xtask,
/// into its own `target/`, and returns the sizes it wrote.
fn build_itself(tree: &Path) -> Result<Sizes, String> {
    read(&size_head(tree)?)
}

/// Runs the tree at `tree`'s own `cargo xtask size --head` and returns the
/// file it wrote: each shipped artifact's size and path ([`write`]).
pub(crate) fn size_head(tree: &Path) -> Result<PathBuf, String> {
    let file = tree.join("target/size/head.tsv");
    let _ = fs::remove_file(&file);
    // rustup's `cargo`, not `$CARGO`, so the tree's rust-toolchain.toml
    // picks its toolchain.
    let status = Command::new("cargo")
        .args(["xtask", "size", "--head"])
        .current_dir(tree)
        .env_remove("RUSTUP_TOOLCHAIN")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET_DIR")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err(format!(
            "`cargo xtask size --head` failed in {}",
            tree.display()
        ));
    }
    Ok(file)
}

/// The paths a shipped build reads; under `crates/`, all but `tests/`.
const INPUTS: [&str; 5] = [
    "crates",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo",
];

/// A name for `rev`'s build inputs: a hash of the blob ids of [`INPUTS`].
fn inputs(root: &Path, rev: &str) -> Result<String, String> {
    let mut args = vec!["ls-tree", "-r", rev, "--"];
    args.extend(INPUTS);
    let listing = git(root, &args)?;
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
    Ok(format!(
        "inputs-{}",
        crate::wasm_ir::source_hash(read.join("\n").as_bytes())
    ))
}

/// The sizes in a `size --head` file ([`write`]).
pub(crate) fn read(file: &Path) -> Result<Sizes, String> {
    let text = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    text.lines()
        .map(|l| {
            let mut f = l.split('\t');
            let (a, b) = f
                .next()
                .zip(f.next())
                .ok_or(format!("{}: {l}", file.display()))?;
            Ok((
                a.to_owned(),
                b.parse().map_err(|e| format!("{}: {e}", file.display()))?,
            ))
        })
        .collect()
}

/// Writes `size --head`'s file: `<artifact> <bytes> <path>`, tab-separated,
/// a line per artifact.
pub(crate) fn write(file: &Path, artifacts: &[(&str, PathBuf)]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut text = String::new();
    for (artifact, path) in artifacts {
        let bytes = crate::wasm::file_size(path)?;
        let _ = writeln!(text, "{artifact}\t{bytes}\t{}", path.display());
    }
    fs::write(file, text).map_err(|e| format!("{}: {e}", file.display()))
}

fn cache_file(cache: &Path, sha: &str) -> PathBuf {
    cache.join(format!("{sha}.tsv"))
}

/// Commit `sha`'s cached sizes under `key`; none when absent or the key
/// differs.
fn load(cache: &Path, sha: &str, key: &str) -> Option<Sizes> {
    let text = fs::read_to_string(cache_file(cache, sha)).ok()?;
    let mut lines = text.lines();
    if lines.next() != Some(&format!("# {key}")) {
        return None;
    }
    lines
        .map(|l| {
            let (a, b) = l.split_once('\t')?;
            Some((a.to_owned(), b.parse().ok()?))
        })
        .collect()
}

fn save(cache: &Path, sha: &str, key: &str, sizes: &Sizes) -> Result<(), String> {
    fs::create_dir_all(cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    let mut text = format!("# {key}\n");
    for (artifact, bytes) in sizes {
        let _ = writeln!(text, "{artifact}\t{bytes}");
    }
    let path = cache_file(cache, sha);
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::{BTreeMap, LOG_FORMAT, Sizes, budgets, git, judge, trailers};
    use std::{env, fs};

    fn sizes(pairs: &[(&str, u64)]) -> Sizes {
        pairs.iter().map(|&(a, b)| (a.to_owned(), b)).collect()
    }

    /// A [`LOG_FORMAT`] record: commit `sha`, its message `body` and its
    /// `Size:` trailer values.
    fn record(sha: &str, body: &str, values: &[&str]) -> String {
        format!("{sha}\x1f{body}\x1f{}\x1d", values.join("\x1e"))
    }

    #[test]
    #[allow(clippy::many_single_char_names, reason = "commits a to d")]
    fn a_rise_needs_trailers_covering_it_and_the_release_a_budget() {
        let release = sizes(&[("hello.wasm", 900), ("hello", 5000)]);
        let base = sizes(&[("hello.wasm", 1000), ("hello", 5000)]);
        let none = BTreeMap::new();
        let rel = Some(("v0.0.1", &release));
        let ok = Vec::<String>::new();
        let t = |log: &str| trailers(log);
        // An unchanged tree passes; one byte past the base without a
        // trailer fails.
        assert_eq!(judge(&base, &base, None, &none, &[]), ok);
        let head = sizes(&[("hello.wasm", 1000), ("hello", 5001)]);
        let failed = judge(&head, &base, None, &none, &t(&record("a", "Fix", &[])));
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert!(failed[0].starts_with("hello grew +1"), "{failed:?}");
        // Another artifact's trailer does not count; one without a reason
        // or a sign is malformed; its own does.
        let other = t(&record("a", "", &["hello.wasm +1 x"]));
        assert_eq!(judge(&head, &base, None, &none, &other).len(), 1);
        for bad in ["hello +1", "hello 1 a table"] {
            let failed = judge(&head, &base, None, &none, &t(&record("a", "", &[bad])));
            assert!(
                failed.iter().any(|f| f.contains("no shipped artifact")),
                "{failed:?}"
            );
        }
        let own = t(&record("a", "", &["hello +1 a table"]));
        assert_eq!(judge(&head, &base, None, &none, &own), ok);
        // Each trailer gives its own commit's rise, so they add up: +600
        // and +500 cover +1,024 though neither alone does; a third commit
        // shrinking it by 100 without a trailer still passes; +600 and
        // +300 do not cover it.
        let head = sizes(&[("hello.wasm", 2024), ("hello", 5000)]);
        let first = record("a", "A", &["hello.wasm +600 a table"]);
        let second = record("b", "B", &["hello.wasm +500 another table"]);
        assert_eq!(judge(&head, &base, None, &none, &t(&first)).len(), 1);
        assert_eq!(judge(&head, &base, None, &none, &t(&second)).len(), 1);
        let both = format!("{second}{first}");
        assert_eq!(judge(&head, &base, None, &none, &t(&both)), ok);
        let shrunk = sizes(&[("hello.wasm", 1924), ("hello", 5000)]);
        let with_shrink = format!("{second}{}{first}", record("c", "Trim a table", &[]));
        assert_eq!(judge(&shrunk, &base, None, &none, &t(&with_shrink)), ok);
        let short = format!("{}{first}", record("c", "C", &["hello.wasm +300 a third"]));
        assert_eq!(judge(&head, &base, None, &none, &t(&short)).len(), 1);
        // Trailers far past the change fail: +999999 covers nothing, and
        // a shrink that leaves more than the slack unused needs its own
        // negative trailer.
        let wide = t(&record("a", "", &["hello.wasm +999999 a table"]));
        let failed = judge(&head, &base, None, &none, &wide);
        assert!(failed[0].contains("add up to +999999"), "{failed:?}");
        let big = sizes(&[("hello.wasm", 31_000), ("hello", 5000)]);
        let rise = record("a", "A", &["hello.wasm +30000 a table"]);
        let fell = sizes(&[("hello.wasm", 21_000), ("hello", 5000)]);
        let cut = format!("{}{rise}", record("b", "B", &[]));
        assert_eq!(judge(&big, &base, None, &none, &t(&rise)), ok);
        assert_eq!(judge(&fell, &base, None, &none, &t(&cut)).len(), 1);
        let cut = format!("{}{rise}", record("b", "B", &["hello.wasm -10000 the cut"]));
        assert_eq!(judge(&fell, &base, None, &none, &t(&cut)), ok);
        // A reverted commit's trailer covers nothing: A +1,024 with its
        // trailer, B reverts A, C rises +1,024 without one: fails. A revert
        // of the revert counts A again.
        let a = record("aaaa", "A", &["hello.wasm +1024 a table"]);
        let b = record("bbbb", "Revert \"A\"\n\nThis reverts commit aaaa.", &[]);
        let c = record("cccc", "C", &[]);
        let reverted = format!("{c}{b}{a}");
        assert_eq!(judge(&head, &base, None, &none, &t(&reverted)).len(), 1);
        let d = record("dddd", "Reapply\n\nThis reverts commit bbbb.", &[]);
        let reapplied = format!("{d}{b}{a}");
        assert_eq!(judge(&head, &base, None, &none, &t(&reapplied)), ok);
        // An artifact renamed or missing against the base or the release
        // fails whatever its size.
        let renamed = sizes(&[("hello2.wasm", 100_000), ("hello", 5000)]);
        let failed = judge(&renamed, &base, rel, &none, &[]);
        assert_eq!(
            failed.iter().filter(|f| f.contains("artifacts")).count(),
            2,
            "{failed:?}"
        );
        // Past the release: over a missing (0) budget fails even with a
        // trailer, within the last budget line for that tag passes; a
        // budget may be signed.
        let why = t(&record("a", "", &["hello.wasm +50 a table"]));
        let head = sizes(&[("hello.wasm", 1050), ("hello", 5000)]);
        let failed = judge(&head, &base, rel, &none, &why);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert!(failed[0].contains("over its +0-byte budget"), "{failed:?}");
        let b =
            budgets("# c\nv0.0.1 hello.wasm 100 D2\nv0.0.1 hello.wasm 150 D3\n").expect("parses");
        assert_eq!(judge(&head, &base, rel, &b, &why), ok);
        let b = budgets("v0.0.1 hello.wasm 149 D2\nv0.0.2 hello.wasm 1000 D9\n").expect("parses");
        assert_eq!(judge(&head, &base, rel, &b, &why).len(), 1);
        let b = budgets("v0.0.1 hello -1000 D4\n").expect("parses");
        assert_eq!(b[&("v0.0.1".to_owned(), "hello".to_owned())], -1000);
        // A shrink past both needs nothing; a budget line needs its reason.
        let head = sizes(&[("hello.wasm", 800), ("hello", 4000)]);
        assert_eq!(judge(&head, &base, rel, &none, &[]), ok);
        assert!(budgets("v0.0.1 hello.wasm 100\n").is_err());
    }

    #[test]
    fn only_a_trailer_git_parses_counts() {
        let dir = env::temp_dir().join(format!("size-trailers-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "--quiet"]).unwrap();
        let commit = |message: &str| {
            let who = [
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "core.hooksPath=/dev/null",
            ];
            let args = [
                &who[..],
                &["commit", "--quiet", "--allow-empty", "-m", message],
            ]
            .concat();
            git(&dir, &args).unwrap();
        };
        commit("base");
        let base = git(&dir, &["rev-parse", "HEAD"]).unwrap();
        commit("A table\n\nSize: hello.wasm +600 a table");
        commit("Quote\n\nIt said:\n\n    Size: hello.wasm +9999 quoted\n\nThat is all.");
        let log = git(&dir, &["log", LOG_FORMAT, &format!("{base}..HEAD")]).unwrap();
        assert_eq!(trailers(&log), vec![("hello.wasm".to_owned(), 600)]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
