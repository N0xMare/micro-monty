//! Accept lines of the instruction-count gates (`native-ir`, `v8-ir`): a
//! commit message line `<Prefix> <names> <ceiling> <reason>` accepts an
//! intended rise of the counts it names, up to its ceiling, and no other.
//!
//! - `<names>`: comma-separated bench programs, or `geomean`; `v8-ir` may
//!   put a guest before one (`plain/dict`, `bundle/geomean`), else the
//!   name covers both guests.
//! - `<ceiling>`: the highest ratio, head over base and head over floor
//!   ([`crate::floors`]), the line passes, such as `1.006`.
//!
//! In the commits since the base a line passes the failures it covers and
//! lets the floors of the counts it names rise to the head's counts, if
//! the rise over the floor is within its ceiling; it moves no other.
//! `MM_*_ACCEPT` with the same text stands for a line while the tree has
//! uncommitted changes; in a clean tree only committed lines count.

use std::path::Path;

use crate::wasm_wall::git;

/// Lines written before the form above, read as what they meant: the
/// commit, its prefix and the line in the current form.
const LEGACY: [(&str, &str, &str); 2] = [
    (
        // v8-ir's first landing: main's plain-guest dict creep since v0.0.1.
        "28ed45dd7fc6aefcfbe1a121c6632d4f036280b1",
        "V8-IR-Accepted:",
        "plain/dict 1.006 correctness costs of cdf5c099 and 901e0377 since v0.0.1",
    ),
    (
        // Written without its names and with the ratio last, `the clearing
        // pass on held, ... (1.0134)`, so wasm-ir could not read the
        // commits since v0.0.2 at all; its native and v8 lines say
        // `held 1.014`.
        "cd4f79b98cd9cbdb56818ce4f3335b50fd307f38",
        "Wasm-IR-Accepted:",
        "held 1.014 the clearing pass on the program that holds suspended generators",
    ),
];

/// One accept line.
#[derive(Debug, Clone)]
pub(crate) struct Accept {
    /// The commit that carries it; empty for `MM_*_ACCEPT`.
    pub(crate) sha: String,
    names: Vec<String>,
    ceiling: f64,
}

impl Accept {
    /// The highest ratio the line passes.
    pub(crate) fn ceiling(&self) -> f64 {
        self.ceiling
    }

    /// Whether the line names `program` (or `geomean`) on `guest`.
    pub(crate) fn names(&self, guest: Option<&str>, program: &str) -> bool {
        self.names
            .iter()
            .any(|n| n == program || guest.is_some_and(|g| n.split_once('/') == Some((g, program))))
    }
}

/// A count past its limit: on `guest` (v8-ir), `name` (a program or
/// `geomean`) at `ratio` against the base or the floor; `text` says so.
pub(crate) struct Failure {
    pub(crate) guest: Option<String>,
    pub(crate) name: String,
    pub(crate) ratio: f64,
    pub(crate) text: String,
}

/// A gate's limits: how far a program's count (`program`, a ratio) and
/// `startup`'s (`startup`, instructions) may rise.
pub(crate) struct Limits {
    pub(crate) program: f64,
    pub(crate) startup: u64,
}

impl Limits {
    /// Whether `name`'s count `head` is past the limit against `other`.
    pub(crate) fn past(&self, name: &str, head: u64, other: u64) -> bool {
        if name == "startup" {
            head > other.saturating_add(self.startup)
        } else {
            ratio(head, other) > self.program
        }
    }
}

#[allow(clippy::cast_precision_loss, reason = "a ratio of counts")]
fn ratio(head: u64, other: u64) -> f64 {
    head as f64 / other.max(1) as f64
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
}
