//! `cargo xtask restack <commit|a..b>...`: cherry-picks commits onto the
//! checkout's HEAD, one at a time, and blesses the golden images again in
//! each commit that moves the snapshot build id (AGENTS.md's snapshot
//! rule), so a stack is rebased or assembled from lanes without resolving
//! the images by hand.
//!
//! Every commit that moves the id rewrites the same two images, so any two
//! lanes conflict there. A pick's conflicts in images both sides changed
//! or added take the parent's; then the golden test runs, and one that
//! fails is blessed (`MM_BLESS=1`) and tested again. A blessed image must
//! carry none of the ids HEAD's images carry, which passed the test: an
//! image blessed again under one of them means the id did not move, so the
//! commit changes what an image means (it needs a `SNAPSHOT_EPOCH` bump,
//! docs/spec.md §9.3), brings a scenario blessed elsewhere, or changes a
//! scenario itself, and `restack` stops. It also stops when blessing
//! writes nothing (a continuation differs from its transcript) and at any
//! other conflict, with the pick staged and uncommitted, and records where
//! in `<git-dir>/RESTACK_STOP`. Once that pick is resolved and staged,
//! `restack --resolved <commit> <rest>...` tests, blesses and commits it
//! as any pick, then goes on. It takes HEAD's image back wherever one is
//! staged with other bytes under HEAD's id, and lets the test judge:
//! blessing under one id need not write the same bytes (a fresh session's
//! heap can differ while the old image still continues the same), so a
//! hand bless is noise when HEAD's image passes and a meaning change when
//! it does not. Only the images whose bytes the bless changed are compared.
//! When the index holds HEAD's image where the picked commit changes one
//! (taken from the parent, kept through a resolution, or taken back), and
//! the test passes but the pick changes `golden.rs`, restack stops too: a
//! cell can change what runs before its park, which only its image holds.
//! A stop that asks for a hand commit prints it whole, and every run
//! refuses a HEAD authored or committed by another identity than
//! AGENTS.md's.
//!
//! Each commit is written by `git commit-tree` with the picked commit's
//! message byte for byte, its author, author date and committer: no hook,
//! message cleanup or local identity touches it. A picked commit authored
//! or committed by another identity than AGENTS.md's is refused before
//! the first pick.
//!
//! The durability results are not recorded per commit: the gate judges the
//! head's file, so they are recorded once, on the stack's tip.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::Write as _,
    path::Path,
    process::{Command, Output, Stdio},
};

use crate::{wasm_wall::git, workspace_root};

const GOLDEN: &str = "crates/micro-monty/tests/golden";
const GOLDEN_RS: &str = "crates/micro-monty/tests/golden.rs";
const USAGE: &str = "usage: cargo xtask restack [--resolved <commit>] <commit|a..b>...";
/// AGENTS.md's author and committer of every commit.
const IDENTITY: &str = "n0x <54751288+N0xMare@users.noreply.github.com>";

/// Each image's snapshot build id by file name (`None`: not an image).
type Ids = BTreeMap<String, Option<[u8; 32]>>;
/// Each image's bytes in the tree, by file name.
type Images = BTreeMap<String, Vec<u8>>;

/// How a run goes on after a stop.
#[derive(PartialEq)]
enum Next {
    /// Run restack again from the stop; with `--resolved` when the pick is
    /// left staged.
    Rerun,
    /// Commit the staged pick by hand, then restack the commits after it.
    ByHand,
    /// `--resolved` (a new scenario blessed elsewhere), or the hand commit
    /// (the scenario itself changed).
    ResolvedOrByHand,
}

/// Why a step stopped, and how a run goes on; a plain error reruns.
struct Halt {
    why: String,
    next: Next,
}

impl From<String> for Halt {
    fn from(why: String) -> Self {
        Self {
            why,
            next: Next::Rerun,
        }
    }
}

/// A stop: why, the commit it stopped at (by index), and how to go on.
struct Stop {
    why: String,
    at: usize,
    next: Next,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let (resolved, rest) = match args {
        [flag, commit, rest @ ..] if flag == "--resolved" => (Some(commit), rest),
        _ => (None, args),
    };
    if (resolved.is_none() && rest.is_empty())
        || rest.iter().chain(resolved).any(|arg| arg.starts_with('-'))
        || resolved.is_some_and(|commit| commit.contains(".."))
    {
        return Err(USAGE.to_owned());
    }
    let root = workspace_root();
    let stop_file = root.join(git(&root, &["rev-parse", "--git-path", "RESTACK_STOP"])?);
    if resolved.is_some() {
        // The tests run on the tree, and the commit takes the index.
        if !git(&root, &["ls-files", "--unmerged"])?.is_empty() {
            return Err("restack --resolved: the index has unmerged paths".to_owned());
        }
        if !git(&root, &["diff", "--name-only"])?.is_empty() {
            return Err("restack --resolved: the tree has changes not staged".to_owned());
        }
        let untracked = git(&root, &["ls-files", "--others", "--exclude-standard"])?;
        if !untracked.is_empty() {
            return Err(format!(
                "restack --resolved: untracked files, which the test would see and the \
                 commit would not hold (`git add` or remove them):\n{untracked}"
            ));
        }
    } else if !git(&root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        return Err("restack: the checkout has uncommitted changes".to_owned());
    } else {
        // A plain run on a clean tree starts afresh: no pick is staged for
        // a record to name. A refused `--resolved`, or a plain run refused
        // above, keeps it, so a mistaken command does not strand a pick.
        let _ = fs::remove_file(&stop_file);
    }
    // A blessed commit takes the whole directory (`git add`).
    let untracked = git(
        &root,
        &["ls-files", "--others", "--exclude-standard", "--", GOLDEN],
    )?;
    if !untracked.is_empty() {
        return Err(format!(
            "restack: {GOLDEN} has untracked files:\n{untracked}"
        ));
    }
    let commits = prepare(&root, resolved, rest, &stop_file)?;
    let start = git(&root, &["rev-parse", "--short", "HEAD"])?;
    println!("restack: {} onto {start}", count(commits.len()));
    let (mut done, mut blessed) = (0, 0);
    if let Err(stop) = restack(&root, &commits, resolved.is_some(), &mut done, &mut blessed) {
        return Err(report(&root, &stop_file, &commits, &start, done, &stop)?);
    }
    let _ = fs::remove_file(&stop_file);
    println!(
        "restack: {}, {blessed} blessed again; record durability once, on the tip: \
         `cargo xtask durability --record` and `--cross-target --record`",
        count(commits.len())
    );
    Ok(())
}

/// A stop's message: its reason, where restack started and HEAD is, the
/// commits left, how to go on and how to go back. Records the stop when the
/// pick is left staged for `--resolved`.
fn report(
    root: &Path,
    stop_file: &Path,
    commits: &[String],
    start: &str,
    done: usize,
    stop: &Stop,
) -> Result<String, String> {
    let head = git(root, &["rev-parse", "HEAD"])?;
    let staged = !git(root, &["diff", "--cached", "--name-only"])?.is_empty();
    let _ = fs::remove_file(stop_file);
    // The stop's own message stays whole when the record cannot be
    // written; `--resolved` then refuses, as it finds no record.
    let mut unrecorded = String::new();
    if staged
        && stop.next != Next::ByHand
        && let Err(e) = fs::write(stop_file, format!("{head} {}\n", commits[done]))
    {
        unrecorded = format!(
            "\nrestack: cannot record the stop in {}: {e}; `--resolved` will refuse to \
             go on, so go back",
            stop_file.display()
        );
    }
    let short = |c: &String| c.get(..12).unwrap_or(c).to_owned();
    let left: Vec<String> = commits[done..].iter().map(short).collect();
    let again = |resolve: bool, from: usize| {
        let rest: Vec<String> = commits[from.min(commits.len())..]
            .iter()
            .map(short)
            .collect();
        if rest.is_empty() {
            "nothing is left to restack".to_owned()
        } else {
            format!(
                "`cargo xtask restack {}{}`",
                if resolve { "--resolved " } else { "" },
                rest.join(" ")
            )
        }
    };
    let go_on = match stop.next {
        Next::Rerun => again(staged, stop.at),
        Next::ByHand => format!(
            "commit it by hand as above, then {}",
            again(false, stop.at + 1)
        ),
        Next::ResolvedOrByHand => format!(
            "{} for a new scenario, or after the hand commit {}",
            again(true, stop.at),
            again(false, stop.at + 1)
        ),
    };
    Ok(format!(
        "{}\nrestack: started at {start}, HEAD is {}: {done} of {} restacked; \
         not restacked: {}. To go on: {go_on}; to go back: `git reset --hard {start}`\
         {unrecorded}",
        stop.why,
        short(&head),
        commits.len(),
        left.join(" "),
    ))
}

/// The commits to restack, once HEAD is AGENTS.md's identity's and, with
/// `--resolved`, the record names HEAD and the commit.
fn prepare(
    root: &Path,
    resolved: Option<&String>,
    rest: &[String],
    stop_file: &Path,
) -> Result<Vec<String>, String> {
    let commits = commits(root, resolved, rest)?;
    // A hand commit, as a stop can ask for, takes the local identity
    // unless told otherwise; restack goes on only on top of the rule's.
    let who = git(
        root,
        &["log", "-1", "--format=%h %s%n%an <%ae>%n%cn <%ce>", "HEAD"],
    )?;
    let [subject, author, committer] = who.lines().collect::<Vec<_>>()[..] else {
        return Err("restack: cannot read HEAD's author and committer".to_owned());
    };
    if author != IDENTITY || committer != IDENTITY {
        // `--reset-author` also takes a new author date: only for another's.
        return Err(format!(
            "restack: HEAD ({subject}) is authored by {author} and committed by {committer}; \
             AGENTS.md's identity is {IDENTITY}. Mend it (`git -c user.name=n0x -c \
             user.email=54751288+N0xMare@users.noreply.github.com commit --amend \
             --cleanup=verbatim --no-edit{}`) before restacking onto it",
            if author == IDENTITY {
                ""
            } else {
                " --reset-author"
            }
        ));
    }
    if resolved.is_some() {
        // The pick staged is the one restack stopped at, on the HEAD it
        // stopped on, which passed the golden test then.
        let head = git(root, &["rev-parse", "HEAD"])?;
        let stop = fs::read_to_string(stop_file).unwrap_or_default();
        if stop.trim() != format!("{head} {}", commits[0]) {
            return Err(format!(
                "restack --resolved: restack did not stop at {} on this HEAD ({head}); it \
                 stopped at `{}` (HEAD, commit)",
                commits[0],
                stop.trim()
            ));
        }
    }
    Ok(commits)
}

fn count(commits: usize) -> String {
    format!("{commits} commit{}", if commits == 1 { "" } else { "s" })
}

/// The commits `resolved` and `args` name, oldest first in each range.
/// Each must have one parent (a merge's own changes would be lost) and
/// AGENTS.md's author and committer, which restack copies.
fn commits(root: &Path, resolved: Option<&String>, args: &[String]) -> Result<Vec<String>, String> {
    let mut commits = Vec::new();
    for arg in resolved.into_iter().chain(args) {
        if arg.contains("...") {
            return Err(format!(
                "restack: `{arg}` holds both sides' commits; give `a..b`"
            ));
        }
        if arg.contains("..") {
            let range = git(root, &["rev-list", "--reverse", arg])?;
            if range.is_empty() {
                return Err(format!("restack: `{arg}` holds no commit"));
            }
            commits.extend(range.lines().map(str::to_owned));
        } else {
            commits.push(git(
                root,
                &["rev-parse", "--verify", &format!("{arg}^{{commit}}")],
            )?);
        }
    }
    for commit in &commits {
        let log = git(
            root,
            &[
                "log",
                "-1",
                "--format=%h %s%n%P%n%an <%ae>%n%cn <%ce>",
                commit,
            ],
        )?;
        let [subject, parents, author, committer] = log.lines().collect::<Vec<_>>()[..] else {
            return Err(format!("restack: cannot read {commit}"));
        };
        let parents = parents.split_whitespace().count();
        if parents != 1 {
            return Err(format!(
                "restack: {subject} has {parents} parents; restack picks commits with one"
            ));
        }
        if author != IDENTITY || committer != IDENTITY {
            return Err(format!(
                "restack: {subject} is authored by {author} and committed by {committer}; \
                 AGENTS.md's identity is {IDENTITY}, and restack copies both"
            ));
        }
    }
    Ok(commits)
}

/// Restacks `commits`, counting into `done` and `blessed`.
fn restack(
    root: &Path,
    commits: &[String],
    resolved: bool,
    done: &mut usize,
    blessed: &mut usize,
) -> Result<(), Stop> {
    // A commit is blessed when the test fails on it, so HEAD must pass,
    // or the first pick would take HEAD's blessing. With `--resolved` the
    // tree is the pick's, and HEAD passed in the run that stopped there.
    if !resolved {
        let stop = |why| Stop {
            why,
            at: 0,
            next: Next::Rerun,
        };
        let head = golden(root, false).map_err(stop)?;
        if !head.status.success() {
            return Err(stop(format!(
                "restack: the golden test fails on HEAD, before any pick; mend HEAD first \
                 (bless it only if the id moved)\n{}",
                output(&head)
            )));
        }
    }
    for (at, commit) in commits.iter().enumerate() {
        let here = |halt: Halt| Stop {
            why: halt.why,
            at,
            next: halt.next,
        };
        let plain = |e: String| here(e.into());
        let subject = git(root, &["log", "-1", "--format=%h %s", commit]).map_err(plain)?;
        // HEAD's images, which passed the test: what the id moved from.
        let before = image_ids(root, Some("HEAD")).map_err(plain)?;
        let how = if resolved && at == 0 {
            take_back_unmoved(root, &subject, &before).map_err(plain)?;
            ", resolved by hand"
        } else if pick(root, commit, &subject).map_err(plain)? {
            ", the images' conflict taken from the parent"
        } else {
            ""
        };
        // Its change is HEAD's already: going on leaves it out.
        holds_change(root, commit, &subject).map_err(|e| Stop {
            why: e,
            at: at + 1,
            next: Next::Rerun,
        })?;
        let moved = bless_if_moved(root, commit, &subject, &before).map_err(here)?;
        // HEAD's images pass, but a scenario's cell can change what runs
        // before the park, which only its image holds: the transcript stays
        // and the test cannot tell.
        if !moved
            && keeps_heads(root, commit).map_err(plain)?
            && !git(
                root,
                &["diff", "--cached", "--name-only", "HEAD", "--", GOLDEN_RS],
            )
            .map_err(plain)?
            .is_empty()
        {
            return Err(Stop {
                why: format!(
                    "restack: {subject}: the golden test passes on HEAD's images, which the \
                     pick holds where the commit changes them, but the pick changes \
                     {GOLDEN_RS}: a scenario's cell may run otherwise before its park, which \
                     only its image holds. If it changes a scenario, {}; else commit it as it \
                     is staged, with that command alone. The pick is staged, uncommitted.",
                    hand_commit(commit)
                ),
                at,
                next: Next::ByHand,
            });
        }
        transcripts_alone(root, &subject).map_err(plain)?;
        commit_as(root, commit).map_err(plain)?;
        *done += 1;
        *blessed += usize::from(moved);
        println!(
            "restack: {subject}: picked{how}{}",
            if moved {
                ", blessed again (the build id moved)"
            } else {
                ""
            },
        );
    }
    Ok(())
}

/// Applies `commit` to the index and the tree, uncommitted. Conflicts in
/// golden images, each changed or added on both sides, are resolved to
/// the parent's images (true), which the golden test then judges; any
/// other conflict stops with it left in the tree.
fn pick(root: &Path, commit: &str, subject: &str) -> Result<bool, String> {
    let picked = Command::new("git")
        .args(["cherry-pick", "--no-commit", commit])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if picked.status.success() {
        return Ok(false);
    }
    let status = git(root, &["status", "--porcelain=v2", "--untracked-files=no"])?;
    // `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>`
    let unmerged: Vec<(&str, &str)> = status
        .lines()
        .filter_map(|line| {
            let mut fields = line.strip_prefix("u ")?.splitn(10, ' ');
            Some((fields.next()?, fields.nth(8)?))
        })
        .collect();
    if unmerged.is_empty() {
        return Err(format!(
            "restack: {subject}: git cherry-pick failed, nothing applied\n{}",
            String::from_utf8_lossy(&picked.stderr).trim()
        ));
    }
    let (images, other): (Vec<_>, Vec<_>) = unmerged.iter().partition(|&&(xy, path)| {
        matches!(xy, "UU" | "AA") && path.starts_with(GOLDEN) && path.ends_with(".mmsnap")
    });
    for (_, path) in &images {
        git(root, &["checkout", "HEAD", "--", path])?;
    }
    if !other.is_empty() {
        let list: Vec<String> = other
            .iter()
            .map(|(xy, path)| format!("  {xy} {path}"))
            .collect();
        return Err(format!(
            "restack: {subject}: conflicts, left in the tree, uncommitted{}:\n{}\n\
             Resolve and `git add` them; `restack --resolved` then tests, blesses and commits \
             the pick as any other.",
            if images.is_empty() {
                ""
            } else {
                " (the images' conflicts are taken from the parent)"
            },
            list.join("\n")
        ));
    }
    Ok(true)
}

/// Stops when the index, which a commit with changes was applied to, is
/// HEAD's tree.
fn holds_change(root: &Path, commit: &str, subject: &str) -> Result<(), String> {
    let tree = git(root, &["write-tree"])?;
    let changes = git(root, &["rev-parse", &format!("{commit}^{{tree}}")])?
        != git(root, &["rev-parse", &format!("{commit}^^{{tree}}")])?;
    if changes && tree == git(root, &["rev-parse", "HEAD^{tree}"])? {
        return Err(format!("restack: {subject}: HEAD already holds its change"));
    }
    Ok(())
}

/// Takes HEAD's image back wherever the staged one has other bytes under
/// an id HEAD's images carry: blessed by hand where the id did not move.
/// The test then judges HEAD's image, and `bless_if_moved` stops a hand
/// bless of a meaning change.
fn take_back_unmoved(root: &Path, subject: &str, before: &Ids) -> Result<(), String> {
    let staged = image_ids(root, None)?;
    let changed = git(
        root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=M",
            "HEAD",
            "--",
            GOLDEN,
        ],
    )?;
    for path in changed.lines() {
        let name = path.rsplit('/').next().unwrap_or(path);
        if let Some(Some(id)) = staged.get(name)
            && before.values().any(|kept| kept == &Some(*id))
        {
            git(root, &["checkout", "HEAD", "--", path])?;
            println!(
                "restack: {subject}: {name} takes HEAD's image back: staged with other bytes \
                 under HEAD's build id, which the golden test judges"
            );
        }
    }
    Ok(())
}

/// Whether the index holds HEAD's image where `commit` changes one: taken
/// from the parent at a conflict, kept through a resolution, or taken back.
/// A new scenario's image and a clean pick's own images are not.
fn keeps_heads(root: &Path, commit: &str) -> Result<bool, String> {
    let changed = git(
        root,
        &[
            "diff",
            "--name-only",
            &format!("{commit}^"),
            commit,
            "--",
            GOLDEN,
        ],
    )?;
    for path in changed.lines().filter(|path| path.ends_with(".mmsnap")) {
        let head = git(
            root,
            &["rev-parse", "--verify", "-q", &format!("HEAD:{path}")],
        );
        let staged = git(root, &["rev-parse", "--verify", "-q", &format!(":{path}")]);
        if let (Ok(head), Ok(staged)) = (head, staged)
            && head == staged
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Says so when the staged change has transcripts and no image: the
/// golden test, which passed, is all that judged them.
fn transcripts_alone(root: &Path, subject: &str) -> Result<(), String> {
    let staged = git(
        root,
        &["diff", "--cached", "--name-only", "HEAD", "--", GOLDEN],
    )?;
    if staged
        .lines()
        .any(|p| Path::new(p).extension().is_some_and(|e| e == "txt"))
        && !staged.lines().any(|p| p.ends_with(".mmsnap"))
    {
        println!("restack: {subject}: changes a golden transcript and no image");
    }
    Ok(())
}

/// Runs the golden test on the pick; if it fails, blesses the images,
/// checks that their id moved from `before` (HEAD's), tests again and
/// stages them (true). Otherwise the images go back to the pick's and it
/// stops.
fn bless_if_moved(root: &Path, commit: &str, subject: &str, before: &Ids) -> Result<bool, Halt> {
    let first = golden(root, false)?;
    if first.status.success() {
        return Ok(false);
    }
    let unblessed = images(root)?;
    let bless = golden(root, true)?;
    let mut next = Next::Rerun;
    let refused = if !bless.status.success() {
        Some(format!(
            "blessing does not mend it (a continuation differs from its transcript, or the \
             build fails)\n{}",
            output(&bless)
        ))
    } else if let Some(id) = unmoved(before, &{
        let wrote = written(&unblessed, &images(root)?);
        let mut ids = image_ids(root, None)?;
        ids.retain(|name, _| wrote.contains(name));
        ids
    }) {
        next = Next::ResolvedOrByHand;
        Some(format!(
            "the build id did not move: blessing writes the images again under an id HEAD's \
             carry ({:02x?}…). The commit either changes what an image means without moving \
             the id, which needs a SNAPSHOT_EPOCH bump (docs/spec.md §9.3); or brings a new \
             scenario whose image was blessed elsewhere: bless, `git add` the new scenario's \
             files only, and go on with `restack --resolved`; or changes a scenario itself \
             (its cell or transcript): {}, and restack the commits left. The test's first \
             failure:\n{}",
            &id[..4],
            hand_commit(commit),
            output(&first)
        ))
    } else {
        let again = golden(root, false)?;
        (!again.status.success())
            .then(|| format!("it still fails after blessing\n{}", output(&again)))
    };
    if let Some(why) = refused {
        // Back to the pick's images; untracked files there are the
        // blessing's, as `run` refuses to start with any.
        git(root, &["checkout", "--", GOLDEN])?;
        git(root, &["clean", "-fq", "--", GOLDEN])?;
        return Err(Halt {
            why: format!(
                "restack: {subject}: the golden test fails, and {why}\n\
                 The pick is staged, uncommitted, with the images as the pick left them."
            ),
            next,
        });
    }
    git(root, &["add", "--", GOLDEN])?;
    Ok(true)
}

/// What a stop asks for when the scenarios changed: bless, stage the
/// directory and commit `commit`'s message as AGENTS.md's identity.
fn hand_commit(commit: &str) -> String {
    format!(
        "bless (`MM_BLESS=1 cargo test --release --locked -p micro-monty --test golden`), \
         `git add {GOLDEN}`, then commit it as AGENTS.md's identity: `git -c user.name=n0x \
         -c user.email=54751288+N0xMare@users.noreply.github.com commit --cleanup=verbatim \
         -C {commit}`"
    )
}

/// The images in the tree whose bytes are not as in `before`, or that are
/// new: the ones a bless run changed. An image it wrote again with the
/// same bytes holds what the failed test already judged, and an image no
/// scenario names is left as it was.
fn written(before: &Images, after: &Images) -> BTreeSet<String> {
    after
        .iter()
        .filter(|(name, bytes)| before.get(*name) != Some(bytes))
        .map(|(name, _)| name.clone())
        .collect()
}

fn images(root: &Path) -> Result<Images, String> {
    let mut images = Images::new();
    let Ok(entries) = fs::read_dir(root.join(GOLDEN)) else {
        return Ok(images);
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".mmsnap") {
            let bytes =
                fs::read(entry.path()).map_err(|e| format!("{}: {e}", entry.path().display()))?;
            images.insert(name, bytes);
        }
    }
    Ok(images)
}

/// A build id that blessing wrote and one of HEAD's images carries, if
/// any, whatever the files' names: the id did not move.
fn unmoved(before: &Ids, after: &Ids) -> Option<[u8; 32]> {
    after
        .values()
        .flatten()
        .find(|id| before.values().any(|kept| kept.as_ref() == Some(*id)))
        .copied()
}

/// The id each image in the golden directory carries, in commit `rev` or
/// in the tree: bytes 10..42 after the magic, as
/// `micro_monty::image_build_id` reads them.
fn image_ids(root: &Path, rev: Option<&str>) -> Result<Ids, String> {
    let paths: Vec<String> = match rev {
        Some(rev) => git(
            root,
            &["ls-tree", "--name-only", rev, "--", &format!("{GOLDEN}/")],
        )?
        .lines()
        .map(str::to_owned)
        .collect(),
        None => fs::read_dir(root.join(GOLDEN)).map_or_else(
            |_| Vec::new(),
            |entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| format!("{GOLDEN}/{}", entry.file_name().to_string_lossy()))
                    .collect()
            },
        ),
    };
    let mut ids = Ids::new();
    for path in paths.iter().filter(|path| path.ends_with(".mmsnap")) {
        let image = match rev {
            Some(rev) => {
                let blob = Command::new("git")
                    .args(["cat-file", "blob", &format!("{rev}:{path}")])
                    .current_dir(root)
                    .output()
                    .map_err(|error| format!("cannot run git: {error}"))?;
                if !blob.status.success() {
                    return Err(format!("restack: cannot read {rev}:{path}"));
                }
                blob.stdout
            }
            None => fs::read(root.join(path)).map_err(|e| format!("{path}: {e}"))?,
        };
        let id = image
            .starts_with(b"MMSNAP\0")
            .then(|| image.get(10..42)?.try_into().ok())
            .flatten();
        let name = path.rsplit('/').next().unwrap_or(path);
        ids.insert(name.to_owned(), id);
    }
    Ok(ids)
}

fn golden(root: &Path, bless: bool) -> Result<Output, String> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut test = Command::new(cargo);
    test.args(["test", "--release", "--locked", "--package", "micro-monty"])
        .args(["--test", "golden"])
        .current_dir(root);
    if bless {
        test.env("MM_BLESS", "1");
    } else {
        test.env_remove("MM_BLESS");
    }
    test.output()
        .map_err(|error| format!("cannot run cargo: {error}"))
}

/// A test run's output, cargo's lines first and the test's last.
fn output(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    )
}

/// Commits the index on HEAD as `picked` was committed: its message's
/// bytes, its author and author date, and its committer.
fn commit_as(root: &Path, picked: &str) -> Result<(), String> {
    let tree = git(root, &["write-tree"])?;
    let who = git(
        root,
        &[
            "log",
            "-1",
            "--format=%an%n%ae%n%ad%n%cn%n%ce",
            "--date=raw",
            picked,
        ],
    )?;
    let [name, email, date, committer, committer_email] = who.lines().collect::<Vec<_>>()[..]
    else {
        return Err(format!(
            "restack: cannot read {picked}'s author and committer"
        ));
    };
    // The message's bytes as the commit object holds them, after the
    // headers' blank line.
    let object = Command::new("git")
        .args(["cat-file", "commit", picked])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    let message = object
        .stdout
        .windows(2)
        .position(|pair| pair == b"\n\n")
        .and_then(|at| object.stdout.get(at + 2..))
        .ok_or_else(|| format!("restack: cannot read {picked}'s message"))?;
    let mut child = Command::new("git")
        .args(["commit-tree", &tree, "-p", "HEAD"])
        .env("GIT_AUTHOR_NAME", name)
        .env("GIT_AUTHOR_EMAIL", email)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_NAME", committer)
        .env("GIT_COMMITTER_EMAIL", committer_email)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run git: {error}"))?;
    child
        .stdin
        .take()
        .ok_or("restack: git commit-tree has no stdin")?
        .write_all(message)
        .map_err(|error| format!("git commit-tree: {error}"))?;
    let commit = child
        .wait_with_output()
        .map_err(|error| format!("git commit-tree: {error}"))?;
    if !commit.status.success() {
        return Err(format!(
            "git commit-tree: {}",
            String::from_utf8_lossy(&commit.stderr).trim()
        ));
    }
    let commit = String::from_utf8_lossy(&commit.stdout).trim().to_owned();
    // Also clears the pick's MERGE_MSG.
    git(root, &["reset", "--soft", &commit])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blessing_under_the_id_an_image_carries_is_no_move() {
        let ids = |entries: &[(&str, Option<u8>)]| -> Ids {
            entries
                .iter()
                .map(|&(name, id)| (name.to_owned(), id.map(|b| [b; 32])))
                .collect()
        };
        let before = ids(&[("sync.mmsnap", Some(1)), ("async.mmsnap", Some(1))]);
        let moved = ids(&[("sync.mmsnap", Some(2)), ("async.mmsnap", Some(2))]);
        assert_eq!(unmoved(&before, &moved), None);
        // One image already carried the new id: it did not move for it.
        let half = ids(&[("sync.mmsnap", Some(2)), ("async.mmsnap", Some(1))]);
        assert_eq!(unmoved(&half, &moved), Some([2; 32]));
        assert_eq!(unmoved(&before, &before), Some([1; 32]));
        // A new image, or one that was not an image, has no id to keep.
        let new = ids(&[("sync.mmsnap", None)]);
        assert_eq!(unmoved(&new, &moved), None);
        // Renamed: the id is compared, not the name.
        let renamed = ids(&[("other.mmsnap", Some(1))]);
        assert_eq!(unmoved(&before, &renamed), Some([1; 32]));
        assert_eq!(unmoved(&ids(&[]), &moved), None);
    }

    #[test]
    fn a_bless_wrote_the_images_whose_bytes_changed() {
        let images = |entries: &[(&str, &[u8])]| -> Images {
            entries
                .iter()
                .map(|&(name, bytes)| (name.to_owned(), bytes.to_vec()))
                .collect()
        };
        let before = images(&[
            ("sync.mmsnap", b"w1"),
            ("async.mmsnap", b"w1"),
            ("old.mmsnap", b"x"),
        ]);
        let after = images(&[
            ("sync.mmsnap", b"w2"),
            ("async.mmsnap", b"w1"),
            ("old.mmsnap", b"x"),
            ("new.mmsnap", b"n"),
        ]);
        // Other bytes of the same length count; the same bytes do not,
        // whatever the file's timestamp.
        let wrote: Vec<String> = written(&before, &after).into_iter().collect();
        assert_eq!(wrote, ["new.mmsnap", "sync.mmsnap"]);
    }
}
