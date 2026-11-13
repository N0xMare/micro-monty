# History

micro-monty was built from 2026-10-31 to its 0.0.1 release on 2026-11-12.
The public repository's history starts at root commit `2bcbb8c`, a squash
of that pre-release history.

## Pre-release history

The pre-release commits are kept in the private archive
`N0xMare/micro-monty-archive`, branch `history-0.0.1`. Commit hashes that
the docs cite, other than those of the public history, are pre-release
history: `CHANGELOG.md`, `docs/sdk.md`, `docs/spec.md`, `docs/research/`,
`docs/spikes/`, the log below and the evidence under `bench/evidence` use
them. The
numbers they qualify are unchanged: a measurement "on `eb78933`" was taken
on that commit.

The archive branch is a rewrite of the commits the docs cite. Its
commits keep the order, trees and author dates, but they have their own
hashes, and their messages use them (`eb78933` is `c897504` there).
[`commits-0.0.1.tsv`](commits-0.0.1.tsv) maps each of the 594 cited-era
commits, from the root to the 0.0.1 release candidate (`e1328bf`, which is
`d6e3008` on the branch), to its commit on `history-0.0.1`. The eight
commits after the release candidate exist only on the branch (`8dc4707`
to `20e8023`).

A few cited hashes are in neither repository: local branches that were
never pushed (the docs name them as such, for example `arch/wasm-lean`
`35c216e`, `size-review/trust` `2bf05d9`, `local/import-image` `0c1b5e9`),
and commits of other projects (Monty, plak, pi, djs, dquickts), which the
docs name with their repository.

## The handoff log

[`handoff-0.0.1.md`](handoff-0.0.1.md) is `docs/HANDOFF.md` as it stood
on 2026-11-12 after the release (6,463 lines): every phase, measurement and
owner decision up to 0.0.1, newest section first. It is kept as it was,
line for line, except two notes added at the end of lines 640 and 778:
`ci.yml`, which those entries describe as the tag gate, was deleted on
2026-11-12. Read it for the evidence behind a decision; `docs/HANDOFF.md`
is the current state and lists the decisions still in force.

References elsewhere to "HANDOFF scope decision N" or to `docs/HANDOFF.md`
"Scope decisions" resolve in both files: the current HANDOFF lists each
decision in force by its number, and the log's "Scope decisions" section
has its full text. References to a dated section (such as "Size pass" or
"Pre-0.1 follow-ups") point here. The comment lines of
`xtask/size-ceilings.txt`, which is append-only, still name
`docs/HANDOFF.md`; the sections they mean are in this log.

## Evidence

Owner decision D1 (a), 2026-11-12: measurement outputs that no gate, test,
xtask command or script reads move to a public `N0xMare/micro-monty-evidence`
repository at the same paths, leaving a README pointer in their place;
earlier commits keep them. Until that repository exists, they stay here
(docs/HANDOFF.md lists the set).
