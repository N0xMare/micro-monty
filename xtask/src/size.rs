//! `cargo xtask size`'s gate: each shipped build against the base's and
//! the release's, all three got on this host (docs/spec.md §6).
//!
//! The base is the merge base of `HEAD` and `--base REV` (default
//! `origin/main`); the release is the latest `v*` tag the base descends
//! from. Each is checked out in a worktree under `target/size/`. A
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
//! - Any growth past the base, one byte or more, needs a trailer
//!   `Size: <artifact> +<bytes> <why>` in a commit since the base whose
//!   `<bytes>` covers at least the measured rise. Growth in a tree with
//!   uncommitted changes always fails: no commit message can cover it.
//! - Growth past the release needs a budget: the last line
//!   `<tag> <artifact> <bytes> <why>` for that release and artifact in
//!   [`BUDGET`], 0 without one. The owner raises it by appending a line.
//!
//! A shrink needs nothing. `xtask/size-ceilings.txt`, the ceilings until
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
    let dir = root.join("target/size");
    let cache = dir.join("cache");
    let key = recipe_key(&root)?;
    let committed = git(&root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty();
    let base = git(&root, &["merge-base", base_rev, "HEAD"])
        .map_err(|e| format!("size: no merge base with {base_rev} to measure against: {e}"))?;
    let release = git(
        &root,
        &["describe", "--tags", "--abbrev=0", "--match", "v*", &base],
    )
    .ok();
    let mut revisions = vec![("base", base.clone(), base.clone())];
    if let Some(tag) = &release {
        revisions.push((
            "release",
            tag.clone(),
            git(&root, &["rev-parse", &format!("{tag}^{{commit}}")])?,
        ));
    }
    let mut sized = Vec::new();
    for (label, name, sha) in revisions {
        let tree = dir.join(format!("{label}-src"));
        let own = builds_itself(&root, &sha);
        let key = if own {
            checkout(&root, &tree, &sha)?;
            println!("size: the {label}, {name}, with its own recipe");
            recipe_key(&tree)?
        } else {
            println!(
                "size: the {label}, {name}, with the head's recipe (its xtask has no `size --head`)"
            );
            key.clone()
        };
        let same = inputs(&root, &sha)?;
        let cached = load(&cache, &sha, &key).or_else(|| load(&cache, &same, &key));
        let got = if let Some(cached) = cached {
            save(&cache, &sha, &key, &cached)?;
            cached
        } else {
            println!("size: building the {label}, {name} (cached afterwards)");
            checkout(&root, &tree, &sha)?;
            let got: Sizes = if own {
                build_itself(&tree)?
            } else {
                build(&tree, &dir.join(label))?
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
    let messages = if committed {
        git(&root, &["log", "--format=%B", &format!("{base}..HEAD")])?
    } else {
        println!("size: uncommitted changes, so no `Size:` trailer counts");
        String::new()
    };
    let budget =
        fs::read_to_string(root.join(BUDGET)).map_err(|e| format!("cannot read {BUDGET}: {e}"))?;
    let budgets = budgets(&budget)?;
    let release = release.as_deref().zip(sized.get(1));
    let failed = judge(&head, &sized[0], release, &budgets, &messages);
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("\n"))
    }
}

/// Why the head's sizes fail against the `base`'s, the `release`'s (its
/// tag and sizes) and that release's `budgets`, given the commit
/// `messages` since the base.
fn judge(
    head: &Sizes,
    base: &Sizes,
    release: Option<(&str, &Sizes)>,
    budgets: &BTreeMap<(String, String), u64>,
    messages: &str,
) -> Vec<String> {
    // The largest rise a `Size: <artifact> +<bytes> <why>` trailer records.
    let covered = |artifact: &str| -> i128 {
        messages
            .lines()
            .filter_map(|l| {
                let mut f = l.trim().strip_prefix("Size:")?.split_whitespace();
                let (a, b, _why) = (f.next()?, f.next()?, f.next()?);
                (a == artifact).then(|| b.strip_prefix('+')?.parse::<i128>().ok())?
            })
            .max()
            .unwrap_or(0)
    };
    let mut failed = Vec::new();
    for (artifact, &bytes) in head {
        let mut line = format!("size: {artifact} {bytes} bytes");
        if let Some(&old) = base.get(artifact) {
            let delta = i128::from(bytes) - i128::from(old);
            let _ = write!(line, ", base {old} ({delta:+})");
            if delta > covered(artifact) {
                failed.push(format!(
                    "{artifact} grew {delta:+} bytes past the base: commit it with the trailer \
                     `Size: {artifact} {delta:+} <why>`"
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
                ", {tag} {old} ({growth:+} of a {allowed}-byte budget)"
            );
            if growth > i128::from(allowed) {
                failed.push(format!(
                    "{artifact} is {growth:+} bytes past {tag}, over its {allowed}-byte budget: \
                     only the owner raises it, with a line `{tag} {artifact} <bytes> <decision>` \
                     in {BUDGET}"
                ));
            }
        }
        println!("{line}");
    }
    failed
}

/// The budgets in `text`: the last `<tag> <artifact> <bytes> <why>` line
/// per tag and artifact.
fn budgets(text: &str) -> Result<BTreeMap<(String, String), u64>, String> {
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
        let bytes = bytes
            .parse()
            .map_err(|_| format!("{BUDGET}:{}: `{bytes}` is not a number of bytes", n + 1))?;
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

/// The cache key of sizes built with the recipe of the tree at `tree`:
/// [`RECIPE`], its toolchain (`rustc -vV` there), `wasm-opt`'s version
/// and a hash of its [`RECIPE_FILES`] as they are in the working tree. A
/// size cached under another recipe is never compared with one built
/// under this one, and a head that lands caches its sizes under the key
/// it gets as the next change's base.
pub(crate) fn recipe_key(tree: &Path) -> Result<String, String> {
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
        .current_dir(tree)
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
#[allow(clippy::expect_used, reason = "tests")]
mod tests {
    use super::{BTreeMap, Sizes, budgets, judge};

    fn sizes(pairs: &[(&str, u64)]) -> Sizes {
        pairs.iter().map(|&(a, b)| (a.to_owned(), b)).collect()
    }

    #[test]
    fn a_rise_needs_a_trailer_covering_it_and_the_release_a_budget() {
        let release = sizes(&[("hello.wasm", 900), ("hello", 5000)]);
        let base = sizes(&[("hello.wasm", 1000), ("hello", 5000)]);
        let none = BTreeMap::new();
        let rel = Some(("v0.0.1", &release));
        let ok = Vec::<String>::new();
        // An unchanged tree passes; one byte past the base without a
        // trailer fails.
        assert_eq!(judge(&base, &base, None, &none, ""), ok);
        let head = sizes(&[("hello.wasm", 1000), ("hello", 5001)]);
        let failed = judge(&head, &base, None, &none, "Fix\n");
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert!(failed[0].starts_with("hello grew +1"), "{failed:?}");
        // Another artifact's trailer, or one without a reason, does not
        // count; its own does.
        assert_eq!(
            judge(&head, &base, None, &none, "Size: hello.wasm +1 x\n").len(),
            1
        );
        assert_eq!(
            judge(&head, &base, None, &none, "Size: hello +1\n").len(),
            1
        );
        assert_eq!(
            judge(&head, &base, None, &none, "x\n\nSize: hello +1 a table\n"),
            ok
        );
        // The trailer must cover the whole rise; the largest one counts.
        let head = sizes(&[("hello.wasm", 2024), ("hello", 5000)]);
        let short = "Size: hello.wasm +1000 a table\n";
        assert_eq!(judge(&head, &base, None, &none, short).len(), 1);
        let two = "Size: hello.wasm +1000 a table\nSize: hello.wasm +1024 the table\n";
        assert_eq!(judge(&head, &base, None, &none, two), ok);
        // Past the release: over a missing (0) budget fails even with a
        // trailer, within the last budget line for that tag passes.
        let why = "Size: hello.wasm +50 a table\n";
        let head = sizes(&[("hello.wasm", 1050), ("hello", 5000)]);
        let failed = judge(&head, &base, rel, &none, why);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert!(failed[0].contains("over its 0-byte budget"), "{failed:?}");
        let b =
            budgets("# c\nv0.0.1 hello.wasm 100 D2\nv0.0.1 hello.wasm 150 D3\n").expect("parses");
        assert_eq!(judge(&head, &base, rel, &b, why), ok);
        let b = budgets("v0.0.1 hello.wasm 149 D2\nv0.0.2 hello.wasm 1000 D9\n").expect("parses");
        assert_eq!(judge(&head, &base, rel, &b, why).len(), 1);
        // A shrink past both needs nothing; a budget line needs its reason.
        let head = sizes(&[("hello.wasm", 800), ("hello", 4000)]);
        assert_eq!(judge(&head, &base, rel, &none, ""), ok);
        assert!(budgets("v0.0.1 hello.wasm 100\n").is_err());
    }
}
