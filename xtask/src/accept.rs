//! Accept lines of the instruction-count gates (`native-ir`, `v8-ir`): a
//! commit message line `<Prefix> <names> <ceiling> <reason>` accepts an
//! intended rise of the counts it names, up to its ceiling, and no other.
//!
//! - `<names>`: comma-separated bench programs, or `geomean`; `v8-ir` may
//!   put a guest before one (`plain/dict`, `bundle/geomean`), else the
//!   name covers both guests.
//! - `<ceiling>`: the highest ratio, head over base and head over anchor,
//!   the line passes, such as `1.006`.
//!
//! In the commits since the base a line passes the failures it covers; in
//! the commits from the anchor (the last `v*` tag) to the base it
//! re-anchors the counts it names: their anchor count is the least from
//! that commit on, so a later rise counts from the accepted level, while
//! the counts it does not name keep the anchor's. `MM_*_ACCEPT` with the
//! same text stands for a line while the tree has uncommitted changes; in
//! a clean tree only committed lines count.

use std::{collections::BTreeMap, path::Path};

use crate::wasm_wall::git;

/// Lines written before the form above, read as what they meant: the
/// commit, its prefix and the line in the current form.
const LEGACY: [(&str, &str, &str); 1] = [(
    // v8-ir's first landing: main's plain-guest dict creep since v0.0.1.
    "28ed45dd7fc6aefcfbe1a121c6632d4f036280b1",
    "V8-IR-Accepted:",
    "plain/dict 1.006 correctness costs of cdf5c099 and 901e0377 since v0.0.1",
)];

/// One accept line.
#[derive(Debug, Clone)]
pub(crate) struct Accept {
    /// The commit that carries it; empty for `MM_*_ACCEPT`.
    pub(crate) sha: String,
    names: Vec<String>,
    ceiling: f64,
}

impl Accept {
    /// Whether the line names `program` (or `geomean`) on `guest`.
    pub(crate) fn names(&self, guest: Option<&str>, program: &str) -> bool {
        self.names
            .iter()
            .any(|n| n == program || guest.is_some_and(|g| n.split_once('/') == Some((g, program))))
    }
}

/// A count past its limit: on `guest` (v8-ir), `name` (a program or
/// `geomean`) at `ratio` against the base or the anchor; `text` says so.
pub(crate) struct Failure {
    pub(crate) guest: Option<String>,
    pub(crate) name: String,
    pub(crate) ratio: f64,
    pub(crate) text: String,
}

/// Parses `<names> <ceiling> <reason>`.
fn parse(sha: &str, value: &str) -> Result<Accept, String> {
    let mut f = value.split_whitespace();
    let (Some(names), Some(ceiling), Some(_reason)) = (f.next(), f.next(), f.next()) else {
        return Err(format!("`{value}` is not `<names> <ceiling> <reason>`"));
    };
    let ceiling: f64 = ceiling
        .parse()
        .ok()
        .filter(|c: &f64| *c >= 1.0 && *c < 2.0)
        .ok_or_else(|| format!("`{ceiling}` in `{value}` is not a ratio such as 1.006"))?;
    Ok(Accept {
        sha: sha.to_owned(),
        names: names.split(',').map(str::to_owned).collect(),
        ceiling,
    })
}

/// The `prefix` lines in the commits of `range` (`A..B`), oldest first.
/// A malformed line fails, naming its commit.
pub(crate) fn committed(root: &Path, prefix: &str, range: &str) -> Result<Vec<Accept>, String> {
    let log = git(root, &["log", "--reverse", "--format=%H%x1f%B%x1d", range])?;
    let mut out = Vec::new();
    for record in log.split('\x1d').map(str::trim).filter(|r| !r.is_empty()) {
        let (sha, body) = record.split_once('\x1f').unwrap_or((record, ""));
        if let Some((_, _, line)) = LEGACY.iter().find(|(s, p, _)| *s == sha && *p == prefix) {
            out.push(parse(sha, line)?);
            continue;
        }
        for line in body.lines() {
            if let Some(value) = line.strip_prefix(prefix) {
                let short = &sha[..sha.len().min(10)];
                out.push(parse(sha, value).map_err(|e| format!("{prefix} in {short}: {e}"))?);
            }
        }
    }
    Ok(out)
}

/// The accept lines for the change since `base`: the committed ones, and
/// `var`'s while the tree has uncommitted changes.
pub(crate) fn for_change(
    root: &Path,
    prefix: &str,
    base: &str,
    var: &str,
) -> Result<Vec<Accept>, String> {
    let mut out = committed(root, prefix, &format!("{base}..HEAD"))?;
    if let Some(value) = std::env::var(var).ok().filter(|v| !v.trim().is_empty()) {
        let dirty = !git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty();
        if dirty {
            out.push(parse("", &value).map_err(|e| format!("{var}: {e}"))?);
        } else {
            println!("{var} is ignored in a clean tree: only a committed `{prefix}` line counts");
        }
    }
    Ok(out)
}

/// The anchor: the latest `v*` tag reachable from `base`, as its name and
/// commit. A tagless or shallow clone fails and says how to fetch it.
pub(crate) fn tag_anchor(root: &Path, base: &str, gate: &str) -> Result<(String, String), String> {
    let tag = git(
        root,
        &["describe", "--tags", "--abbrev=0", "--match", "v*", base],
    )
    .map_err(|_| {
        format!(
            "{gate}: no v* tag reachable from {base}, so no anchor: run `git fetch --tags \
             --unshallow origin` (or `git fetch --tags origin`), or name one with --anchor REV"
        )
    })?;
    let sha = git(
        root,
        &["rev-parse", "--verify", &format!("{tag}^{{commit}}")],
    )?;
    Ok((tag, sha))
}

/// Each count's anchor: its least over `history` (the anchor first, then
/// the commits after it, the base last, each a commit and its counts),
/// from the latest of `accepts` that names it on. `split` turns a count's
/// key into its guest and program.
pub(crate) fn least_since(
    root: &Path,
    history: &[(&str, &BTreeMap<String, u64>)],
    accepts: &[Accept],
    split: impl Fn(&str) -> (Option<String>, String),
) -> Result<BTreeMap<String, u64>, String> {
    let (Some(oldest), Some(newest)) = (history.first(), history.last()) else {
        return Ok(BTreeMap::new());
    };
    // Each commit's position on the first-parent line from the anchor.
    let mut position = BTreeMap::from([(oldest.0.to_owned(), 0_usize)]);
    let list = git(
        root,
        &[
            "rev-list",
            "--first-parent",
            "--reverse",
            &format!("{}..{}", oldest.0, newest.0),
        ],
    )?;
    for (i, sha) in list.lines().enumerate() {
        position.insert(sha.to_owned(), i + 1);
    }
    let at = |sha: &str| position.get(sha).copied();
    let mut out = BTreeMap::new();
    for key in history.iter().flat_map(|(_, c)| c.keys()) {
        if out.contains_key(key) {
            continue;
        }
        let (guest, program) = split(key);
        let start = accepts
            .iter()
            .filter(|a| a.names(guest.as_deref(), &program))
            .filter_map(|a| at(&a.sha))
            .max()
            .unwrap_or(0);
        // The build at `start` is the last side at or before it.
        let from = history
            .iter()
            .filter_map(|(s, _)| at(s))
            .filter(|&p| p <= start)
            .max()
            .unwrap_or(0);
        let least = history
            .iter()
            .filter(|(s, _)| at(s).is_some_and(|p| p >= from))
            .filter_map(|(_, c)| c.get(key).copied())
            .min();
        if let Some(n) = least {
            out.insert(key.clone(), n);
        }
    }
    Ok(out)
}

/// Passes when every failure is covered by one of `accepts` (a line that
/// names it with a ceiling at or above its ratio); else fails with the
/// line to write.
pub(crate) fn verdict(
    gate: &str,
    prefix: &str,
    var: &str,
    failures: &[Failure],
    accepts: &[Accept],
) -> Result<(), String> {
    if failures.is_empty() {
        println!("\n{gate}: no count rose past its limit");
        return Ok(());
    }
    let mut open = Vec::new();
    for f in failures {
        let by = accepts
            .iter()
            .find(|a| a.names(f.guest.as_deref(), &f.name) && f.ratio <= a.ceiling);
        match by {
            Some(a) if a.sha.is_empty() => println!("{gate}: {}, accepted by {var}", f.text),
            Some(a) => println!(
                "{gate}: {}, accepted by {}",
                f.text,
                &a.sha[..a.sha.len().min(10)]
            ),
            None => open.push(f),
        }
    }
    if open.is_empty() {
        return Ok(());
    }
    let mut names: Vec<String> = Vec::new();
    for f in &open {
        let n = f
            .guest
            .as_ref()
            .map_or_else(|| f.name.clone(), |g| format!("{g}/{}", f.name));
        if !names.contains(&n) {
            names.push(n);
        }
    }
    let top = open.iter().map(|f| f.ratio).fold(1.0, f64::max);
    let ceiling = (top * 1000.0).ceil() / 1000.0;
    Err(format!(
        "{gate}: past the limits: {}; if the rise is intended, put the table and the line \
         `{prefix} {} {ceiling:.3} <reason>` in the commit message ({var} with the same text \
         stands for it before committing)",
        open.iter()
            .map(|f| f.text.as_str())
            .collect::<Vec<_>>()
            .join("; "),
        names.join(",")
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests fail on a broken fixture")]

    use super::*;

    fn failure(guest: Option<&str>, name: &str, ratio: f64) -> Failure {
        Failure {
            guest: guest.map(str::to_owned),
            name: name.to_owned(),
            ratio,
            text: format!("{name} {ratio}"),
        }
    }

    #[test]
    fn a_line_passes_only_what_it_names_up_to_its_ceiling() {
        let line = parse("abc", "plain/dict,fib 1.006 why").unwrap();
        assert!(line.names(Some("plain"), "dict"));
        assert!(!line.names(Some("bundle"), "dict"));
        assert!(line.names(Some("bundle"), "fib") && line.names(None, "fib"));
        let ok = |f: Failure| verdict("g", "G:", "V", &[f], std::slice::from_ref(&line)).is_ok();
        assert!(ok(failure(Some("plain"), "dict", 1.0059)));
        assert!(!ok(failure(Some("plain"), "dict", 1.0061)));
        assert!(!ok(failure(Some("bundle"), "dict", 1.0059)));
        assert!(!ok(failure(Some("plain"), "closure", 1.0021)));
        assert!(parse("abc", "dict why").is_err());
        assert!(parse("abc", "dict 6% why").is_err());
        let error = verdict("g", "G:", "V", &[failure(None, "dict", 1.0051)], &[]).unwrap_err();
        assert!(error.contains("`G: dict 1.006 <reason>`"), "{error}");
    }

    #[test]
    fn an_accept_re_anchors_only_what_it_names() {
        let dir = std::env::temp_dir().join(format!("accept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
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
            git(&dir, &["rev-parse", "HEAD"]).unwrap()
        };
        let (a, b) = (commit("anchor"), commit("b"));
        commit("c\n\nG: dict 1.004 a correctness cost");
        let d = commit("base");
        let c = |fib: u64, dict: u64| {
            BTreeMap::from([("fib".to_owned(), fib), ("dict".to_owned(), dict)])
        };
        let (ca, cb, cd) = (c(100, 100), c(99, 103), c(101, 106));
        let history = [(a.as_str(), &ca), (b.as_str(), &cb), (d.as_str(), &cd)];
        let accepts = committed(&dir, "G:", &format!("{a}..{d}")).unwrap();
        let least = least_since(&dir, &history, &accepts, |k| (None, k.to_owned())).unwrap();
        // dict counts from the last build before the accepted commit, b;
        // fib keeps its least since the anchor.
        assert_eq!(least["dict"], 103);
        assert_eq!(least["fib"], 99);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
