# CPython 3.14 `Lib/test` on micro-monty: core modules (2026-11-07)

The core-module pass: more of CPython 3.14.7's own test suite run on mm
the way the asyncio batches were (`docs/HANDOFF.md` "asyncio batch 3"),
the failures sorted by first cause, and the real bugs that models
plausibly hit fixed, each with an own case that matches python3.14.

## Method

- Harness: `conformance/breadth/cpython-tests` (`harness.py`, `report.py`,
  shim) unchanged, on `Lib/test` of CPython 3.14.7 (inputs and commands in
  its README). A method counts for mm only when it passes on CPython with
  the same shim and the same transformed source.
- Units: the 125 of `modules.txt` (the asked-for ones are all in it:
  the `test_json` package members, `test_re`, `test_collections`,
  `datetimetester`, `test_str`, `test_string.test_string`, `test_dict`,
  `test_list`, `test_set`, `test_itertools`, `test_functools`,
  `test_dataclasses`, `test_enum`, `test_math`, `test_statistics`,
  `test_decimal`, `test_textwrap`, `test_difflib`), plus the bundled
  modules models use that have a test module but were not listed:
  `test_calendar`, `test_codecs`, `test_hmac`, `test_htmlparser`,
  `test_zoneinfo.test_zoneinfo` (and `test_warnings`, which passes nothing
  on CPython under the shim). These six ran from an extended copy of
  `modules.txt`; the recorded `results/` keep the 125 units of
  `modules.txt`, so the 2026-11-04 and later numbers stay comparable.
- Before: `main` at 266bde7 (engine as db2521a with the `mod-asyncio`/
  `mod-json` gates). After: this pass. Both runs on this orb, 12 jobs.
- Causes: the harness's first-cause classification (`report.py`), read
  per method; out of scope means `docs/spec.md` §4/§12 excludes it
  (`exec`/`eval`/`compile`, `__slots__`/`__del__`, files, threads,
  `weakref`, `inspect`, `pickle`, C-API details, `difflib.HtmlDiff`).

## Result [M]

All units (130 run, 11,218 methods): **9,613 pass on CPython; mm 6,108 →
6,405** (+297), no method lost (one random-driven `test_sort` method,
`test_bug453523`, passes or fails by chance on both builds: mm detects a
list mutated during `sort` in about 60% of runs, CPython always).

The recorded 125 units (`conformance/breadth/cpython-tests/results/`):
8,989 pass on CPython, mm 5,964 → **6,261** with this pass (66.3% →
69.7%), 6,274 on `main` with the capability gaps it was rebased on (+13 in
test_fstring and test_exceptions; 75.5% without the 678 deliberately
excluded methods). The requested units are the same on both.

### Requested units

| unit | methods | pass CPython | mm before | mm after |
|---|---:|---:|---:|---:|
| test_json.test_decode | 38 | 18 | 13 | 16 |
| test_json.test_default | 6 | 3 | 1 | 3 |
| test_json.test_dump | 17 | 8 | 6 | 8 |
| test_json.test_encode_basestring_ascii | 7 | 3 | 1 | 2 |
| test_json.test_enum | 12 | 6 | 0 | 6 |
| test_json.test_fail | 14 | 7 | 5 | 6 |
| test_json.test_float | 8 | 4 | 4 | 4 |
| test_json.test_indent | 4 | 2 | 2 | 2 |
| test_json.test_pass1/2/3 | 6 | 3 | 3 | 3 |
| test_json.test_recursion | 12 | 5 | 1 | 5 |
| test_json.test_scanstring | 8 | 4 | 0 | 0 |
| test_json.test_separators | 4 | 2 | 2 | 2 |
| test_json.test_unicode | 34 | 17 | 7 | 9 |
| test_re | 165 | 149 | 121 | 121 |
| test_collections | 101 | 97 | 57 | 76 |
| datetimetester | 553 | 502 | 437 | 449 |
| test_str | 139 | 125 | 96 | 97 |
| test_string.test_string | 42 | 41 | 18 | 18 |
| test_dict | 121 | 110 | 80 | 87 |
| test_list | 68 | 64 | 55 | 56 |
| test_set | 630 | 626 | 513 | 547 |
| test_itertools | 136 | 111 | 99 | 99 |
| test_functools | 325 | 198 | 102 | 115 |
| test_dataclasses | 280 | 266 | 198 | 199 |
| test_enum | 1077 | 1068 | 860 | 908 |
| test_math | 88 | 81 | 67 | 73 |
| test_statistics | 371 | 345 | 320 | 324 |
| test_decimal | 368 | 159 | 120 | 151 |
| test_textwrap | 68 | 68 | 66 | 66 |
| test_difflib | 39 | 38 | 0 | 0 |
| **total** | 4741 | 4130 | 3254 | 3452 |

### Other units that moved, and the added bundled units

| unit | methods | pass CPython | mm before | mm after |
|---|---:|---:|---:|---:|
| test_ordered_dict | 178 | 140 | 98 | 108 |
| test_userdict | 28 | 28 | 20 | 26 |
| test_userlist | 54 | 53 | 41 | 48 |
| test_userstring | 71 | 69 | 52 | 65 |
| test_typing | 699 | 677 | 200 | 223 |
| test_abc | 72 | 72 | 26 | 32 |
| test_int | 40 | 32 | 12 | 26 |
| test_long | 47 | 38 | 27 | 32 |
| test_fractions | 50 | 50 | 36 | 38 |
| test_descr, test_super, test_compare, test_types, test_builtin, test_tuple, test_bytes, test_tomllib.test_misc | | | | +2, +2, +2, +2, +1, +1, +1, +1 |
| test_calendar (added) | 79 | 61 | 60 | 60 |
| test_htmlparser (added) | 67 | 65 | 65 | 65 |
| test_codecs (added) | 287 | 270 | 18 | 18 |
| test_hmac (added) | 18 | 18 | 1 | 1 |
| test_zoneinfo.test_zoneinfo (added) | 252 | 210 | 0 | 0 |

## Bugs fixed (each an own case matching python3.14)

| own case | what was wrong |
|---|---|
| 400 | `json.dumps` raised "not JSON serializable" for `IntEnum`, `StrEnum`, `OrderedDict`, `defaultdict`, `Counter` and other builtin-type subclasses; now encoded natively as the value they extend (a `dict` subclass with its own `items` goes to the Python encoder). Errors carry CPython's `__notes__` (`default=`, bad keys, NaN with `allow_nan=False`, circular references); `skipkeys` with `indent` gives `{}`; `JSONEncoder`/`JSONDecoder` classes with `cls=`, `raw_decode`, `strict=False`; `json.encoder`/`json.decoder`; a complete `\uXXXX` at the end of input is "Unterminated string" |
| 401 | PEP 560 `__mro_entries__`: `class C(Generic[T])`, `class C(Mapping[str, int])` and other non-class bases (typing's aliases), `__orig_bases__` |
| 402 | `str.__str__` on a str subclass, `__repr__`/`__str__`/`__format__` returning a str subclass, str-subclass attribute names in `getattr` and co., `format()` of ReprEnum mixins (`IntEnum`, `StrEnum`, `IntFlag` format as their value), `object.__format__('')` honouring a user `__str__` |
| 403 | a container subclass overriding only `__getitem__` (OrderedDict LRU caches) lost native `in`/iteration; `ChainMap`, `UserDict`, `UserList`, `UserString` methods and operators; `str % x` with `x`'s `__rmod__`; a set inside a set/frozenset; set-subclass operators |
| 404 | `__get__` of functions, classmethod, staticmethod and property called directly; `__isabstractmethod__`; `partialmethod`/`singledispatchmethod` binding |
| 405 | the `__class__` cell of a class built by a metaclass (`super()` in its methods); enum's "do not use `super().__new__`" |
| 406 | `a != b` skipped `b`'s reflected `__ne__` after `a.__eq__` returned `NotImplemented` |
| 407 | builtin iterators (`list_iterator`, `dict_keyiterator`, `enumerate`, `zip`, ...) had no `__next__`/`__iter__` methods (`it.__next__()`, `iter(xs).__next__` as a callable) |
| 408 | `dict.update`/`dict()` on a user object did not call `keys()` first (its exception became "not iterable"); `fromkeys` on a dict subclass returned a plain dict |
| 409 | `sys.set_int_max_str_digits`/`get_int_max_str_digits`/`int_info` missing although the limit was enforced and its message names them; the limit lives in the prelude's globals, so a snapshot keeps it |
| 410 | `re.IGNORECASE` sets missed Unicode case pairs (Kelvin sign in `[J-M]`/`[19K]`, long s in `[a-z]`) |
| 411 | `math.prod`/`sumprod` refused `Fraction`/`Decimal` ("does not support user-defined objects", also through `statistics.geometric_mean`/`linear_regression`); `math.dist` with `__float__` objects; `comb`/`perm`/`isqrt`/`factorial` refused int subclasses (`IntEnum`), `isinf`/`floor`/`sqrt` float subclasses |
| 412 | `ord(b'a')` raised TypeError; `copy`/`deepcopy` of a `timezone` or tz-aware `datetime` failed (no `object.__getstate__`, then `ord` on bytes) |
| 413 | `type(name, (SomeABC,), ns)` ignored the base's metaclass, so ABCs stayed abstract and user metaclasses did not run |
| 414 | `', '.join([Color.RED, ...])` with StrEnum members (str subclasses) raised "expected str instance"; a format spec's `0` after an explicit alignment (`<08`) padded with spaces |
| 415 | str methods and `in` refused a str-subclass argument: `status in text`, `startswith`, `replace`, `split`, `find`, `count`, `partition`, `strip`, `removeprefix`, `ljust` fill; error messages named the argument wrongly ("must be str must be str") |
| 416 | IntEnum/IntFlag members as slice bounds, in `hex`/`bin`/`oct`, `bytes(n)` and `find`/`index` start/end |

## What remains, by first cause

Out of scope (spec §4, §12, deliberate): `pickle` (datetimetester 31,
test_set 13, test_dict 6, ...), `eval`/`exec`/`compile` (about 30 across
the units), `__slots__`/`__del__` (test_dataclasses 13 + 7, test_dict 7,
test_functools 4), `weakref`, `threading`, `inspect`, files and
`tempfile` (test_zoneinfo's 210 all create a temporary directory),
`difflib.HtmlDiff` (all 38 test_difflib methods fail in `setUpModule`,
which sets `HtmlDiff._default_prefix`), internals the tests reach into
(`json.decoder.scanstring`, `re._compiler`/`re._constants`, `_strptime`,
`_operator`, `datetime._divide_and_round`, module namespace cleanup),
`sys.modules`, and lone surrogates (`chr(0xD800)`: mm's strings are UTF-8;
`test_re.test_bigcharset` builds a class over U+0100–U+FFFF).

Real, not fixed (rare in model code, or large):

- **test_re** (28 left): `memoryview` (4) and `bytearray` subjects (2) are
  unsupported; three ignore-case methods also need CPython's extra-case
  table (`ﬅ`/`ﬆ`, `[\u2129-\u212b]` vs `k`); `\N{...}` (1); `m.group()`
  with an `__index__` object (1); `re.Scanner` (1); `locale` (1); one
  look-behind case times out.
- **test_collections** (21): `memoryview` and `bytearray` registrations
  in the ABCs (4); `dir()` of `dict`/`list`/`str` lacks names the
  `UserDict`/`UserList`/`UserString` dirs have (3); `set.__sub__`,
  `property.__doc__`, `function.__globals__` as attributes; a
  `namedtuple` past mm's 126-parameter limit; a ChainMap subclass's `|`
  returns a plain ChainMap.
- **datetimetester** (53): 31 pickle, 7 eval, 4 files; 4 bad-state
  "backdoor" inputs reach an unbound local instead of CPython's
  `TypeError`; `_strptime` imported directly (3).
- **test_str** (28): codecs mm does not have (`utf-7`, `cp037`, `idna`,
  `unicode-escape`, `raw-unicode-escape`: 6), `str.__mod__`/`__mul__`/
  `__getnewargs__` as attributes (4), `'%s' % s` and `'{}'.format(s)` on a
  str subclass with its own `__str__`, `''.join([sub])` returning the same
  object, huge-width and huge-index formats.
- **test_string** (23): `string.Formatter` (14) and `Template` subclasses
  that override `idpattern`/`braceidpattern`/`flags`/`pattern` (9): mm's
  Template scans without `re`.
- **test_dict** (23): beyond the out-of-scope ones, `dict.__or__` called
  explicitly, `dict_keys.mapping`, the 3.14 "object is not iterable" message
  with a note for `dict([1])`, and two mutation-during-iteration checks.
- **test_set** (79): subclassing `frozenset` (36: mm refuses it at class
  creation), `set_iterator.__length_hint__` (7), recursive-set `repr` of
  subclasses.
- **test_itertools** (12): `zip` subclasses, `tee` internals, sizes past
  `ssize_t`.
- **test_functools** (83): `functools.Placeholder` (10, new in 3.14),
  `function.__annotations__` (5, annotations are never evaluated),
  `singledispatch` registration from annotations (at least 13: needs
  evaluated annotations), `partial` repr of subclasses, `_c3_mro`.
- **test_dataclasses** (67): beyond `__slots__`, the generated `__doc__`
  (12: CPython builds it with `inspect.signature`), `annotationlib` (6),
  string annotations (`'int'` vs `int`, `ClassVar` as a string).
- **test_enum** (160): reserved member names (`mro`, `_create_`: 23), a
  32-method group asserting internals (`_generate_next_value_` kept as a
  `staticmethod` in the class dict, `_is_descriptor`, IntFlag basics
  among them),
  `globals()` writes by `_convert_` (9), `_add_alias_`, `EnumDict`,
  `_simple_enum`, `DynamicClassAttribute`, pickling (8).
- **test_math** (8): `nextafter(steps=)`, `fma` with infinities, the
  `__ceil__`/`__floor__` descriptor protocol (2), files (2).
- **test_statistics** (21): `sys.modules` (18), `super().__rtruediv__`.
- **test_decimal** (8): eval (5), pickle (2).
- **test_textwrap** (2): `\N{...}` escapes.
- **test_codecs** (252 of 270): incremental and stream codecs
  (`getreader`/`getwriter`/`getincrementaldecoder`: about 100), the
  `encodings` package and the codecs mm lacks; the spec scopes `codecs` to
  the registries over `str.encode` (§4).
- **test_hmac** (17): `_operator`, and `compare_digest` as a Python
  function binds as a method where the tests store it on a class.

## Cost [M]

Bench callgrind Ir vs 012435b, the `main` this lands on (`bench/callgrind.py`,
`mm run`, stripped binaries at equal-length paths, outputs identical):
startup +0.429% (+3,839 Ir), fib +0.001%, loop 0.000%, dict 0.000%, str
−0.057%, json +0.335%, closure +0.001%, classes 0.000%, generators 0.000%,
exceptions +0.041%, fmt +0.052%, sort +0.012%, interp +0.211%. Vs 266bde7
before the rebase: startup +0.206%, fib
+0.001%, loop 0.000%, dict 0.000%, str +0.171%, json +0.338%, closure
0.000%, classes −0.044%, generators −0.139%, exceptions −0.056%, fmt
+0.042%, sort −0.043%, interp +0.199%. A first version measured json
+1.47%, str +0.46% and fmt +0.43%; the subclass, key and join paths moved
out of line (`json::subclass_value`, `json::key_text`,
`strmeth::join_values`, `ops::backed_slice_index`) took them back.

Size vs 266bde7 (`cargo xtask size`, shipped builds): `hello.wasm`
1,999,470 → 2,024,218 (+24,748), native `hello` 2,177,048 → 2,202,776
(+25,728), guest 2,103,826 → 2,128,589 (+24,763). The prelude and stdlib
Python changes are about 6.8 KB of it (an ablation with only the Rust
changes, on a revision before the cold paths moved out of line:
`hello.wasm` +19,582 of +26,376); the rest is engine code, the largest pieces
`build_resolved` (`__mro_entries__`), `join_values`, `find_module`
(the json submodules), `json::subclass_value`/`key_text`, `re::one` and
`any_abstract`. Ceilings raised to the measurement plus about 2 KB (owner
decision S1). Rebased on the capability gaps (traceback, `ast`,
`tokenize`), the pass adds +25,388 / +26,352 / +25,410 to 2,091,289 /
2,276,696 / 2,195,383; ceilings 2,093,300 / 2,278,700 / 2,197,400.
