# Differential fuzz of main (fec4ff6) against CPython 3.14.7

One 40-minute run of `fuzz/difffuzz/difffuzz.py` (the grammar-based
generator, now with async-for/async-with cleanup blocks) on the frozen
main binary. This was a measurement only: no engine, corpus or fuzzer file
was changed. Numbers are [M] unless marked otherwise.

## Run

```bash
~/.local/bin/python3.14 fuzz/difffuzz/difffuzz.py \
    --mm /tmp/mm-evidence-layers/main/mm --python /home/user/.local/bin/python3.14 \
    --jobs 4 --minutes 40 --seeds 10000000 --start 0 \
    --out conformance/breadth/difffuzz/raw
```

- Binary: `/tmp/mm-evidence-layers/main/mm`, SHA-256
  `ec95bd9e7dc963747fbb205f7ef2da3438543f329cd429778e99e47af91f8b8b`
  (the same hash `artifacts.sha256` records for the fec4ff6 build).
- CPython 3.14.7 (`~/.local/bin/python3.14`), run as `python3.14 -X utf8`
  with `PYTHONHASHSEED=0`. The CPython timeout is 10 s and the mm timeout
  is 40 s (the defaults).
- Wall time: 2026-11-04 21:22:03Z to 22:02:04Z (`real 40m1.6s`). The
  fuzzer's own count is 2402 s. Other heavy jobs were running on the
  16-core machine at the same time.
- Seeds were processed in roughly the order 0..44,2xx (`imap_unordered`).
- **44,224 programs generated and run**: 43,374 identical, **630
  divergent (593 signatures)**, 220 skipped because CPython timed out.
  Among the 44,004 programs CPython finished, mm never timed out or
  crashed: every divergent program exits 0 in both engines.
- The fuzzer minimizes up to 2 programs per signature. Here 626 minimized
  divergences are in `raw/` (2.5 MB). `run.log` is the fuzzer's stdout and
  stderr with 40 CPython `SyntaxWarning: 'return'/'break' in a 'finally'
  block` lines removed. It contains the per-signature counts.

## Divergences by root cause

Classes: (a) real semantic bug in mm, (b) deliberate documented exclusion
or limit, (c) message or formatting only, (d) fuzzer or oracle artifact.
Signatures are blanked of digits, so one root cause produces many of them;
each of the 593 was assigned to exactly one group below. The per-group
counts add up to 630 divergences and 593 signatures.

| # | Group | Divergences (signatures) | Class | Minimized repro | CPython | mm |
|---|---|---|---|---|---|---|
| 1 | `repr` does not escape unassigned (Cn) and other non-printable code points | 604 (575) | (a) | `repr-unassigned-codepoint.py`: `print(repr(chr(0x378)))` | `'\u0378'` | `'͸'` (raw U+0378) |
| 1′ | same, as seen in error text (`int()`/`float()` messages embed `repr`) | (included in 1) | (a) | `repr-unassigned-in-error-message.py`: `int(chr(0x1aec))` | `invalid literal for int() with base 10: '\u1aec'` | `... '᫬'` |
| 2 | `pow` preflight `MemoryError` on huge powers (`2 ** 64 ** k`, `3 ** 50 ** k`) | 13 (11) | (b) | `pow-memoryerror-below-limit.py`: `print((2 ** (2 ** 29)).bit_length())` | `536870913` | `MemoryError` |
| 3 | `print` writes nothing when `str(int)` hits the 4300-digit limit; CPython has already written the earlier arguments | 9 (4) | (a), minor | `print-int-str-limit-partial-output.py`: `print("a", 10 ** 5000)` in `try` | `a EXC` | `EXC` |
| 4 | `sorted`/`list.sort` compare in a different order (merge sort, not CPython's timsort) | 3 (2) | (a), seen by the fuzzer as (c) | `sort-comparison-order.py`: `sorted([0, 0, 'a', 0])` | `'<' not supported between instances of 'str' and 'int'` | `... 'int' and 'str'` |
| 5 | `int / int` with a subnormal quotient is off by 1 ulp | 1 (1) | (a) | `int-truediv-subnormal-rounding.py`: `print(6 / (int(1e300) << 30))` | `5.587935447692873e-309` | `5.58793544769287e-309` |

No divergence is class (c) only or class (d). Group 4 shows up in the fuzzer
only as a reversed type order in the message, but its cause is semantic (see
below). The CPython timeouts are (d) and are counted separately.

### 1. `repr` escaping of non-printable code points: (a)

- Cause: `text::is_printable` (`crates/mm-engine/src/text.rs:360-367`,
  doc comment "An approximation of `str.isprintable`") lists only C0/C1,
  U+00AD, some spaces and format characters, surrogates and private use.
  `str.isprintable` uses the full table (`strmeth.rs:593`, `ucd::printable`)
  and returns `False` correctly for these characters. Over all non-surrogate
  code points, mm's `repr` and mm's `isprintable` disagree on 819,682 of them,
  almost all unassigned (Cn).
- In the fuzzer the characters come from `chr(N)`. The effect spreads
  wherever `repr` is used: container printing, `!r`, `%r`, exception
  messages (`int()`, `float()`, `KeyError`), and `len`/`ord`/iteration over
  `repr(...)` text, which accounts for the few non-obvious signatures (seeds
  16054, 17327, 34419, 36628, 37598, 12236).
- Not a documented exclusion: `docs/HANDOFF.md:1740` lists "nonprintable
  Unicode in `int(chr(6892))` error text" as remaining conformance work.
- The earlier lost run's "about 20 divergences in repr escaping of
  non-ASCII text" **reproduces**: it is this group. At 44k programs it
  accounts for 604 of the 630 divergences.

### 2. Huge powers: (b)

- `docs/spec.md:1027` (§11 Limits): heap memory is host-set (plak: 256 MiB),
  with "preflight before big allocations (`*` repeat, `pow`, `<<`, ...);
  raises a catchable `MemoryError`". `docs/sdk.md:120` says the same
  (`Limits::memory`, default 256 MiB). Every saved repro contains
  `2 ** 64 ** (x % 7)` or `3 ** 50 ** (x % 7)`.
- Follow-up worth noting, not a bug by the letter of the spec: the preflight
  is conservative. `2 ** (2 ** 29)` (a 64 MiB result) raises `MemoryError`,
  while `1 << (2 ** 29)`, which gives the same value, succeeds, and so does
  `2 ** (2 ** 28)`.

### 3. `print` partial output: (a), minor

CPython's `print` writes each argument and separator as it goes, so
`"finally "` is on stdout before `str(10 ** 5000)` raises `ValueError`.
mm raises before writing anything. With a user `__str__` that raises,
mm already matches CPython (`print("a", "b", C(), "d")` writes `a b`), so
only the native int-to-text path renders everything first. Every saved repro
is `print("label", <3 ** 50 ** k>)`.

### 4. Sort comparison order: (a), low impact

mm's native sort is a merge sort (`crates/mm-engine/src/builtins.rs:2891`),
not CPython's timsort with binary insertion. For a valid total order the
results are equal. What differs:

- which pair fails first in mixed-type lists. Of the 996 erroring lists in
  `itertools.product([0, 1, 2, 'a'], repeat=2..5)`, 150 get a reversed
  message;
- the sequence of `__lt__` calls. `sorted([K(2), K(1), K(3)])` makes 4 calls
  in CPython (`(1,2) (3,1) (1,3) (3,2)`) and 3 in mm;
- the result for non-total orders. Sorting 3,000 random lists of frozensets
  gave a different order in 497 of them, for example the second line of
  `sort-comparison-order.py`.

### 5. Subnormal `int / int`: (a), low impact

`6 / (int(1e300) << 30)`: the exact quotient is 0.40 ulp from CPython's
result and 0.60 ulp from mm's, so mm did not round correctly (probably
double rounding into the subnormal range). Over 20,000 generated
int/int quotients, 196 of the 14,079 subnormal results were 1 ulp off and
no normal-range result was (two similar checks with 60,000 normal-range
quotients found 0 differences).

## CPython timeouts (skipped, class (d))

220 programs were skipped because CPython took more than 10 s. They are not
divergences, and the fuzzer does not record them by seed, so I re-ran
CPython alone on seeds 0..44,299 with the same 10 s timeout to list them. The earlier lost run's 5 timeouts
**reproduce in kind**. The count depends on machine load and run length.

The re-run found 224 timeouts, against 220 during the fuzz run. The
difference is the 3 load-sensitive seeds below plus seeds above the last
one the run reached. For each seed, `faulthandler` recorded the line
CPython was on after 9 s (`cpython-timeouts.txt`, one line per seed):

| Cause | Seeds |
|---|---|
| Generator precedence: `BIGS` literals are inserted without parentheses (`difffuzz.py:37`, `:114`) into `({a} ** ({b} % 7))` (`:118`), so `x ** (10 ** 30 + 7 % 7)` is `x ** 10 ** 30` and `x ** ((1 << 100) - 1 % 7)` is `x ** ((1 << 100) - 1)` | 161 |
| Same precedence issue: `2 ** 64 ** (k % 7)` is `2 ** (64 ** k)`, up to `2 ** 2 ** 36` | 40 |
| Precedence pattern elsewhere in the program (unary-prefixed variants) | 5 |
| Programs that never finish or are very slow (each timed out after 30 s when rerun alone). Seed 3696 is a `while n9 > 0` loop whose body reassigns `n9 = ....bit_length()` | 15 |
| Load only: finished under 9 s when rerun | 3 |

All of these are class (d): the generated program is not a bounded test.
mm finishes the precedence cases in 0.01 s with a catchable `MemoryError`
(checked on seeds 65, 193 and 1021). The same generator precedence issue is
what triggers groups 2 and 3: only `2 ** 64 ** k` and `3 ** 50 ** k` reach
those sizes. So group 2 is a documented limit reached through a generator
artifact, and group 3 is a real but minor mm difference that the artifact
exposed.

## Comparison rule for libm results (added 2026-11-06)

Later runs found 6 divergences that are libm artifacts, not mm bugs: glibc
or Apple libm is 1 ulp off where mm is correctly rounded [S] (the Mac
thread's difffuzz report): `147 ** -3` (CPython `3.148096204560773e-07`,
mm `3.1480962045607726e-07` on glibc [M]), `math.log2(0.5984721441039565)`,
`math.sin(2.6683328128252666)`, and `math.sin(2.5)` on Apple only. Since
then `difffuzz.py` (its module docstring has the rule) wraps every
generated libm result (`math.exp/log/log2/log10/sin/atan`, float `**`,
`int ** -k`) in `_T`. On a divergence both engines rerun with `_T`
snapping those results to a 4-ulp grid and recording them; the program
counts as `ulp`, not divergent, when the snapped outputs agree and every
recorded pair is equal or 1 ulp apart. Exact operations (arithmetic,
`sqrt`, `fabs`, int `**`, `round`, formatting) are not wrapped and must
still agree exactly: a program with a 1-ulp libm difference and a real
difference elsewhere stays divergent. The rule changes generated program
text (the `_T` line and wrappers) but not which program a seed draws.

## Files

- `README.md`: this report.
- `repr-unassigned-codepoint.py`, `repr-unassigned-in-error-message.py`,
  `pow-memoryerror-below-limit.py`, `print-int-str-limit-partial-output.py`,
  `sort-comparison-order.py`, `int-truediv-subnormal-rounding.py`: the
  minimized repros. Run each with `python3.14 -X utf8 F` and `mm run F`
  to compare.
- `raw/seed-N.py`: the fuzzer's minimized divergences, with both outputs in
  the header.
- `run.log`: the fuzzer's output, including the signature table.
- `cpython-timeouts.txt`: each CPython-timeout seed with its cause and the
  line CPython was executing.
