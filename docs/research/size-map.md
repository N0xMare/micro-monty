# Size map of the shipped builds

Commit hashes of micro-monty cited here, other than the public history's
(root `2bcbb8c`), are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`;
[docs/history](../history/README.md) maps them); the numbers they qualify
are unchanged.

Measured 2026-11-07 on `a72abfe`. Rust 1.99.0, binaryen 133, twiggy 0.8.0.
Labels: [M] measured, [E] estimated. Every number can be reproduced with
`xtask/size_map.py` (see "Method").

## Summary

- **The shipped builds are about three-quarters code and one-quarter data.**
  hello.wasm (wasm32-unknown-unknown) is 2,101,121 bytes: code 1,535,637
  and data 561,525 [M]. The native `hello` (2,303,928) has about the same
  code in `.text` (1,632,205) and 627 KB of data in `.rodata` plus
  `.data.rel.ro`. Its `.rela.dyn` (38,376) relocates the pointer tables at
  load under PIE [M].
- **Data is mostly Python bytecode.** It is 81% of the wasm data
  [M]: 386,198 B for the 63 stdlib modules and 67,124 B for the prelude,
  of which asyncio is 24,958 and `dir` name lists 10,137. Strings,
  core's float and unicode tables and the builtin table make up the rest.
- **Code by crate (hello.wasm, counted where the source was written):**

  | crate | bytes |
  |---|---:|
  | mm-engine | 846 K |
  | mm-core | 270 K |
  | mm-compile | 138 K |
  | std/core/alloc, out of line | 128 K |
  | mm-parse | 106 K |
  | dlmalloc and mm-wasm-alloc | 32 K |
  | facade | 11 K (45 K in the guest) |

  [M] by line tables, scaled through wasm-opt [E]. mm-core is large because
  its value, heap and arena accessors are inlined at every use: 270 KB
  inlined against 29 KB of its own functions.
- **Capabilities that can be switched off today** (removal builds
  [M], hello.wasm / native):

  | capability | hello.wasm | native |
  |---|---:|---:|
  | all of the stdlib except struct and operator | −455,094 | −471,856 |
  | datetime | −49,228 | −54,112 |
  | re | −44,745 | −47,904 |
  | decimal | −39,863 | −39,936 |
  | hashlib | −27,647 | −29,744 |
  | unicodedata, which is opt-in | +376,451 | +391,104 |

  The `mod-asyncio` and `mod-json` features remove nothing: asyncio
  (about 106 KB of code and prelude [M attribution]) and json (about 28 KB)
  are always built in.
- **The largest language features are always present** [M attribution,
  hello.wasm]:

  | feature | bytes |
  |---|---:|
  | compiler | 118 K |
  | parser | 109 K |
  | str methods and formatting | 109 K |
  | asyncio | 106 K |
  | snapshots | 71 K |
  | classes (class.rs) | 70 K |
  | bytes | 56 K |
  | math and libm | 52 K |
  | exceptions | 37 K |
  | complex numbers | 31 K |
  | big ints | 29 K |
  | match statement | 28 K |
  | generators and coroutines | 18 K |
  | dunder dispatch | 19 K |
  | GC | 16 K |

  Overlaps and dependencies are under "Capabilities"; for example, the
  prelude and 15 stdlib modules use `yield`, and 36 stdlib modules
  subclass.
- **Free savings** ("Removable without losing capability or speed"):
  - `NATIVES` borrows a field of the `PACKED` const, so the whole
    compile-time builtin table ships: 47,168 B natively and 42,496 B of
    wasm memory. Fixing it saves about 33 KB native and about 5 KB of
    wasm file [E from M]. In wasm the zero runs are already left out of
    the file.
  - `hello` is built with the facade's `test-hooks` feature, unified in
    from its dev-dependency: −312 / −176 B [M] when it is off. This only
    affects the measurement; embedders don't get the hooks.
  - Monomorphization is small. Copies beyond the first in each generic
    family total 26 KB in hello.wasm and 43 KB natively, and `drop_glue` is
    the largest family (8 KB / 11 KB). No duplicated large function was
    found.

  Everything else that is large is either a capability or inlined hot-path
  code, where size competes with speed: `Vm::slot`/`raw`/`hx`/`str_ref`
  inlined at every use come to about 115 KB [M attribution].

## Method

`uv run --with pyelftools python3 xtask/size_map.py` builds the four artifacts
into `target/size-map/` exactly as `cargo xtask size` does
(`shipped_cargo` in `xtask/src/main.rs`: `RUSTC_BOOTSTRAP=1`,
`-Zbuild-std=std,panic_abort`, `-Cpanic=immediate-abort`,
`-Cforce-unwind-tables=no`, release profile, `hello` with its own crate at
`-Copt-level=3`, `wasm-opt -Oz` with the shipped feature list). Only three
settings differ: `strip = false`, `debug = "line-tables-only"`, and a linker
map (`--Map` for wasm-ld, `-Wl,-Map` natively). The script writes
`target/size-map/report/size-map.json`, a `size-map.md` with every table
below and more (top 300 functions, all files, statics), and twiggy's
`top`, `dominators`, `monos`, `garbage` and `paths` for each wasm module. `--removals` adds the removal
builds, stripped as shipped, in a git worktree.
`--markdown` re-renders the tables from the JSON. A full run takes
about 8 minutes, and the removal builds about 40.

**Fidelity [M].** The named builds carry the shipped code:

| build | named build without name section | shipped |
|---|---:|---:|
| hello.wasm uu | 2,101,364 | 2,101,121 |
| hello.wasm wasip1 | 2,111,663 | 2,111,336 |
| guest | 2,205,368 | 2,205,141 |

That is +0.012% to +0.015%. Before wasm-opt the code sections differ by 1 B and the data
sections have the same length. The difference is the order of constants
and strings, which follows the crate metadata hashes (the profile settings
are part of them), not codegen: a build with names but without line tables
differs from shipped by the same amount. `wasm-opt -Oz -g` gives the same
bytes as `wasm-opt -Oz` once the name section is removed. Natively,
`.rodata`, `.data.rel.ro` and `.data` are the same size as shipped, and
`.text` differs by 368 B. The removal builds are the shipped build:
their baseline is 2,101,121 / 2,303,928 bytes, as `cargo xtask size`.

**Attribution.**
- *Code by function*: the wasm name section after wasm-opt, and `nm`
  natively. A function belongs to the crate and module of the item or impl
  block that defines it: v0 symbols are parsed, so `<Vm>::execute` written
  in `mm_engine::interp` belongs to `interp`. A generic std function
  belongs to std.
- *Code by source file*: the DWARF line tables, with the inline call sites
  (`DW_TAG_inlined_subroutine`). Each byte goes to the innermost frame
  written in this repository, so a `?`, `Vec::push` or `Option::unwrap_or`
  inlined into `class.rs` counts for `class.rs`. std keeps only the bytes of
  its out-of-line functions. A second view, "user", skips mm-core's
  inlined accessors and charges the calling engine file. For wasm the line
  tables describe the module before wasm-opt, so per-file numbers are
  scaled by the code wasm-opt kept: 1,535,637 / 1,693,131 = 0.907 [E].
  Scaling per function would misplace code that wasm-opt inlines into its
  callers.
- *Capabilities*: the bytes whose inline stack contains a frame from the
  capability's functions. That includes helpers inlined into them, their
  inlined copies in callers and, for the interpreter, the `op::…` arms of
  `dispatch`/`dispatch_rare`. It is what deleting those functions would
  remove [M as attribution]. It is not a removal build: capabilities
  overlap, and other code may need them.
- *Data*: twiggy sees only opaque segments, so the script places every
  byte of the data segments itself:
  - Blobs are found by content: each stdlib module's packed bytecode and
    their shared header (mm-stdlib's OUT_DIR), the prelude chunks
    (mm-engine's OUT_DIR) and `src/ucd/*.bin`. All were found in every
    build.
  - Named statics and jump tables come from the link map.
  - Anonymous constants, including lld's merged string pool, go to the crate
    whose code holds their address as an `i32.const` (wasm only, by byte
    scan [E]). Each is then classified as strings or other constants.
  - Natively, anonymous constants stay "?".
  - The prelude is split per `# ---- section` of `prelude.py`. Each unit's
    record length is charged at its chunk's packing ratio [E per section,
    M in total].

## Totals [M]

| build | shipped | code | data | other sections |
|---|---:|---:|---:|---:|
| hello.wasm, wasm32-unknown-unknown | 2,101,121 | 1,535,637 | 561,525 | 4,202 |
| hello.wasm, wasm32-wasip1 | 2,111,336 | 1,543,540 | 563,523 | 4,600 |
| mm_wasm_guest.wasm, wasm32-unknown-unknown | 2,205,141 | 1,634,520 | 565,965 | 4,883 |
| native hello, x86_64 | 2,303,928 | 1,632,205 (`.text`) | 627,631 (`.rodata` 564,647, `.data.rel.ro` 62,816, `.data` 168) | 44,092 (`.rela.dyn` 38,376, `.dynsym`/`.dynstr`, headers) |

The code and data columns come from the named builds, which are within
0.015% of shipped. On hello.wasm, wasm-opt takes code from 1,693,131 to 1,535,637 (−9.3%) and
data from 591,604 to 561,525 bytes in the file, because zero runs are not
stored. The name section, which is not shipped, is 120,920 B.

## Code by crate

Line tables, inlined code counted where it was written ([M]; wasm scaled by
wasm-opt's overall ratio [E]):

| crate | hello.wasm uu | hello.wasm wasip1 | guest uu | native hello |
|---|---:|---:|---:|---:|
| mm-engine | 845,641 | 861,677 | 857,581 | 925,778 |
| mm-core | 270,237 | 270,771 | 270,751 | 278,524 |
| mm-compile | 138,100 | 138,760 | 137,748 | 141,520 |
| std/core/alloc (out-of-line functions) | 128,077 | 147,479 | 136,466 | 149,737 |
| mm-parse | 106,002 | 106,191 | 105,749 | 109,029 |
| dlmalloc + mm-wasm-alloc | 31,518 | 0 (wasi-libc malloc, in std's share) | 33,448 | 0 (glibc) |
| facade (micro-monty, hello's `main`) | 10,967 | 13,115 | 45,192 | 14,618 |
| mm-wasm-abi | | | 29,692 | |
| mm-wasm-guest | | | 12,538 | |
| no line info | 5,087 | 5,533 | 5,353 | 12,631 |

By the function that holds the code [M], the picture differs only because of
inlining. In hello.wasm, `hello::main` is one 191,250 B function: LTO and
wasm-opt inline `Session::start`, `Vm::execute`, `dispatch` and
`dispatch_rare` into it. So "facade" holds 198 KB by name, but its own code
is 11 KB. In the guest, `Vm::execute` is out of line (77,193 B), and so are
`mm_session_restore` (45,061 B), `mm_define` (23,714 B) and `mm_snapshot`
(19,006 B). mm-core owns only 29 KB (hello.wasm) and 39 KB (native) as
out-of-line functions; the rest of its 270 KB is inlined accessors.

## Code by source file

Top files by line tables [M] (wasm scaled [E]). The "user" columns
charge mm-core's inlined accessors to the calling engine file. That is why
`vm.rs` jumps there: `Vm::slot`, `raw`, `hx`, `str_ref`, `set_slot` and
`set_raw` are thin `#[inline]` wrappers over the heap.

| file | hello.wasm uu | guest uu | native | hello.wasm uu, user view | native, user view |
|---|---:|---:|---:|---:|---:|
| mm-engine builtins.rs | 94,992 | 94,804 | 91,287 | 100,149 | 95,947 |
| mm-core space.rs | 94,804 | 94,513 | 94,283 | 17,438 | 18,751 |
| mm-compile compiler.rs | 79,280 | 79,128 | 81,886 | 79,280 | 81,886 |
| mm-core heap.rs | 70,918 | 71,523 | 74,977 | (in callers) | (in callers) |
| mm-engine interp.rs | 68,398 | 68,880 | 80,989 | 81,887 | 96,175 |
| mm-parse parser.rs | 67,572 | 67,395 | 74,134 | 67,572 | 74,134 |
| mm-core value.rs | 59,769 | 59,972 | 60,011 | (in callers) | (in callers) |
| mm-engine vm.rs | 58,075 | 60,373 | 65,272 | 199,326 | 208,533 |
| mm-engine snapshot.rs | 43,103 | 41,748 | 47,400 | 59,825 | 66,005 |
| mm-engine text.rs | 41,761 | 41,684 | 44,538 | 43,084 | 46,084 |
| mm-engine ops.rs | 41,629 | 41,547 | 42,399 | 46,847 | 46,703 |
| mm-engine bytes.rs | 36,379 | 36,307 | 40,644 | 38,026 | 42,281 |
| mm-engine aio.rs | 33,207 | 34,437 | 38,829 | 38,167 | 43,720 |
| mm-engine strmeth.rs | 27,850 | 27,794 | 30,418 | 28,323 | 30,884 |
| mm-engine class.rs | 27,562 | 27,507 | 30,584 | 34,580 | 37,409 |
| mm-engine bigint.rs | 27,319 | 27,265 | 29,884 | 27,319 | 29,884 |
| mm-engine exc.rs | 26,000 | 25,949 | 32,782 | 26,875 | 33,301 |
| mm-engine libm.rs | 23,960 | 23,912 | 28,436 | 23,960 | 28,436 |
| mm-wasm-alloc lib.rs (dealloc/alloc call sites) | 22,506 | 24,325 | – | 22,506 | – |
| mm-engine dict.rs | 20,419 | 20,378 | 23,133 | 33,586 | 35,452 |
| mm-compile symtable.rs | 20,037 | 20,013 | 17,832 | 20,037 | 17,832 |
| mm-core string.rs | 19,300 | 19,361 | 19,694 | (in callers) | (in callers) |
| mm-engine walk.rs | 17,591 | 17,556 | 15,563 | 20,095 | 18,752 |
| mm-engine re.rs | 17,563 | 17,528 | 20,738 | 17,639 | 20,774 |
| mm-engine intops.rs | 17,449 | 17,415 | 19,395 | 19,235 | 20,946 |
| mm-core snapshot.rs | 17,426 | 17,377 | 20,629 | (in callers) | (in callers) |
| mm-engine math.rs | 17,171 | 17,137 | 17,921 | 19,040 | 19,994 |
| mm-engine host.rs | 15,636 | 24,089 | 19,885 | 16,016 | 20,332 |
| mm-parse lexer.rs | 15,451 | 15,426 | 15,168 | 15,451 | 15,168 |
| mm-engine json.rs | 15,129 | 15,099 | 16,901 | 17,258 | 18,974 |
| mm-engine hashlib.rs | 14,988 | 14,958 | 16,105 | 15,023 | 16,141 |
| mm-compile compiler/matching.rs | 14,964 | 14,935 | 16,296 | 14,964 | 16,296 |
| mm-engine complex.rs | 12,310 | 12,286 | 12,962 | 13,455 | 14,389 |
| mm-engine dunder.rs | 12,275 | 12,251 | 14,540 | 12,927 | 14,890 |
| mm-engine datetimemod.rs | 12,086 | 12,062 | 12,805 | 12,134 | 12,874 |
| mm-engine percent.rs | 11,584 | 11,561 | 13,395 | 11,783 | 13,556 |
| mm-engine cmath.rs | 10,690 | 10,668 | 11,382 | 10,737 | 11,427 |
| mm-engine scratch.rs | 9,705 | 9,685 | 11,469 | 10,266 | 11,976 |
| mm-parse walk.rs | 9,050 | 9,032 | 5,679 | 9,050 | 5,679 |
| mm-engine modules.rs | 9,006 | 9,007 | 11,890 | 9,184 | 12,090 |
| mm-engine structmod.rs | 8,923 | 8,905 | 10,395 | 8,987 | 10,621 |
| mm-engine cont.rs | 8,687 | 8,670 | 8,080 | 9,939 | 8,854 |
| mm-engine prelude.rs | 8,459 | 8,443 | 8,214 | 8,508 | 8,288 |
| mm-compile encode.rs | 8,247 | 8,231 | 10,082 | 8,247 | 10,082 |
| mm-core gc.rs | 7,759 | 7,744 | 8,795 | (in callers) | (in callers) |
| mm-engine set.rs | 6,764 | 6,751 | 8,684 | 8,544 | 9,964 |

The largest inlined helpers, from the user view by Rust function
(hello.wasm, [M] attribution, scaled [E]):

| inlined helper | bytes |
|---|---:|
| `vm.rs::slot` | 52,695 |
| `vm.rs::raw` | 32,846 |
| `builtins.rs::arg` | 20,316 |
| `vm.rs::hx` (heap error to exception) | 17,233 |
| mm-wasm-alloc `dealloc` call sites | 15,386 |
| `vm.rs::str_ref` | 13,162 |
| `vm.rs::set_raw` | 11,056 |
| `vm.rs::set_slot` | 9,899 |
| `vm.rs::int_of` | 7,109 |
| `vm.rs::alloc_tuple` | 5,942 |

## Data by owner

[M]. For wasm these are bytes in the file. Anonymous constants are
assigned to crates by the code that refers to them [E]. The wasm total is the
payload of the data segments; the section adds about 900 B of segment
headers.

| owner | hello.wasm uu | guest uu | native hello |
|---|---:|---:|---:|
| stdlib bytecode: 63 modules plus the shared Huffman header (158) | 386,198 | 386,198 | 386,198 |
| prelude bytecode | 67,124 | 67,124 | 67,124 |
| builtins.rs `PACKED`, the whole compile-time table (see candidates) | 14,712 (42,496 in memory) | 14,712 | 47,168 (`.data.rel.ro`) |
| string constants, mm-engine (messages, names; includes lld's merged pool) | 33,154 | 33,366 | (crate not assigned natively) |
| string constants, mm-parse, mm-compile, facade, guest | 7,159 | 10,691 | |
| string constants, crate not assigned | 5,836 | 5,767 | 47,339 (all strings) |
| core statics (dec2flt `POWER_OF_FIVE_128` 10,131; unicode case/alphabetic tables ~4.5 KB; grisu `CACHED_POW10` 1,277) | 16,413 | 16,413 | 20,828 |
| mm-engine named statics and jump tables (`prelude::UNITS` 2,910, `T::name` 555, …) | 5,730 | 5,866 | 17,961 |
| other constants: vtables, numeric and dispatch tables | 17,024 | 17,017 | 13,190 |
| other pointer tables in `.data.rel.ro` | | | 14,880 |
| `printable.bin` (`str.isprintable`, `repr`) | 2,982 | 2,982 | 2,982 |
| rest (crc table 512, zero runs, small tables of other crates) | 4,294 | 4,897 | 9,793 |
| **total** | **560,626** | **565,033** | **627,463** |

**Stdlib modules, packed** [M]. The bytes are identical in all four builds:

| module | bytes | module | bytes | module | bytes |
|---|---:|---|---:|---|---:|
| decimal | 39,698 | functools | 7,383 | heapq | 2,116 |
| datetime | 26,719 | difflib | 7,207 | string | 2,052 |
| re | 22,163 | pprint | 7,001 | fnmatch | 1,994 |
| collections | 14,577 | datetime._strptime | 6,091 | zoneinfo._common | 1,941 |
| statistics | 13,834 | os | 5,627 | numbers | 1,839 |
| enum | 11,377 | pathlib | 5,553 | html | 1,837 |
| urllib.parse | 11,367 | html.parser | 5,497 | bisect | 1,508 |
| dataclasses | 10,916 | os.path | 5,326 | tomllib._re | 1,411 |
| logging | 10,420 | contextlib | 5,298 | zoneinfo._tzpath | 1,331 |
| time | 10,226 | base64 | 5,206 | platform | 1,104 |
| itertools | 9,219 | warnings | 4,754 | tempfile | 775 |
| random | 9,008 | traceback | 4,545 | gc | 764 |
| fractions | 8,858 | binascii | 4,446 | zlib | 751 |
| csv | 8,576 | operator | 4,027 | posixpath | 598 |
| typing | 8,530 | shlex | 3,650 | zoneinfo | 405 |
| calendar | 8,502 | codecs | 3,422 | tomllib._types | 139 |
| tomllib._parser | 8,311 | textwrap | 3,350 | tomllib | 135 |
| collections.abc | 7,959 | copy | 3,311 | urllib | 18 |
| zoneinfo._zoneinfo | 7,655 | _markupbase | 3,212 | header | 158 |
| hashlib | 7,543 | io | 2,841 | | |
| | | types | 2,806 | | |
| | | struct | 2,595 | | |
| | | abc | 2,274 | | |
| | | contextvars | 2,262 | | |
| | | hmac | 2,180 | | |

**Prelude, by section of `prelude.py`** (packed; per-section split [E], total
67,124 [M]):

| section | bytes |
|---|---:|
| asyncio | 24,958 |
| dir (CPython's attribute names of builtin types) | 10,137 |
| GenericAlias, Union | 8,055 |
| subscripts | 4,892 |
| builtins | 3,558 |
| json | 3,493 |
| repr | 3,340 |
| equality | 2,210 |
| match | 1,588 |
| sys | 1,488 |
| keys | 1,192 |
| host records | 692 |
| metaclasses | 474 |
| reporting | 448 |
| slot wrappers | 271 |
| classes | 171 |
| header | 158 |

## Capabilities

The bytes each language feature or module costs in hello.wasm uu and native
hello.

- *Code* counts the bytes whose inline stack touches the capability's
  functions ([M] as attribution; wasm scaled [E]). Rows overlap, for
  example asyncio with generators, so they don't add up.
- *Removal* is a stripped shipped build with that mm-stdlib feature
  dropped from the default set and from every other feature's list [M].
  Modules that import it still build, so the removal measures the module's
  own cost.
- Removal comes out 2–4 KB above code plus bytecode. The difference is
  strings, the builtin table entries and the Huffman header refitted to
  fewer modules.

| capability | code wasm | code native | prelude | stdlib bytecode | wasm total (attribution) | removal wasm [M] | removal native [M] |
|---|---:|---:|---:|---:|---:|---:|---:|
| parser (mm-parse) | 109,050 | 111,724 | | | 109,050 | | |
| compiler (compiler*, symtable) | 117,558 | 118,068 | | | 117,558 | | |
| bytecode encode/decode, packing | 24,590 | 26,787 | | | 24,590 | | |
| GC: mark, sweep, collect | 16,331 | 18,610 | | | 16,331 | | |
| classes, all of class.rs | 69,043 | 70,688 | 645 | | 69,688 | | |
| · inheritance: C3, lookup along the MRO, class creation | 16,906 | 18,532 | | | 16,906 | | |
| · `super()` | 2,929 | 2,864 | | | 2,929 | | |
| · descriptors: property, classmethod, staticmethod | 4,419 | 3,994 | | | 4,419 | | |
| dunder dispatch on user objects (dunder.rs) | 18,362 | 18,233 | 271 | | 18,633 | | |
| generators and coroutines: generator.rs, YIELD/AWAIT/SEND arms, genexp | 17,657 | 19,734 | | | 17,657 | | |
| asyncio: aio.rs, `aio_*` builtins, prelude | 80,710 | 86,520 | 24,958 | | 105,668 | 0 (feature gates nothing) | −16 |
| regex (re.rs, re.py) | 19,552 | 22,236 | | 22,163 | 41,715 | −44,745 | −47,904 |
| big integers (bigint.rs) | 29,162 | 29,890 | | | 29,162 | | |
| json (json.rs, prelude) | 24,511 | 26,741 | 3,493 | | 28,004 | +16 (feature gates nothing) | 0 |
| datetime (datetimemod.rs, datetime, _strptime) | 14,174 | 14,934 | | 32,810 | 46,984 | −49,228 | −54,112 |
| zoneinfo (Python) | | | | 11,332 | 11,332 | −10,126 | −10,320 |
| decimal (Python) | | | | 39,698 | 39,698 | −39,863 | −39,936 |
| unicodedata (opt-in tables and code) | | | | | | +376,451 | +391,104 |
| hashlib (hashlib.rs, hashlib.py) | 15,945 | 16,870 | | 7,543 | 23,488 | −27,647 | −29,744 |
| random (random.rs, random.py) | 6,129 | 6,719 | | 9,008 | 15,137 | −15,954 | −17,296 |
| binascii | 6,077 | 5,816 | | 4,446 | 10,523 | −12,838 | −13,616 |
| struct (structmod.rs, struct.py) | 10,388 | 11,926 | | 2,595 | 12,983 | build fails without it | |
| statistics, enum, dataclasses, typing (Python) | | | | 13,834 / 11,377 / 10,916 / 8,530 | | −13,863 / −11,471 / −10,951 / −8,641 | −13,952 / −11,536 / −11,040 / −8,704 |
| all stdlib modules except struct and operator | | | | about 379,600 | | −455,094 | −471,856 |
| snapshots (engine and mm-core snapshot.rs) | 71,476 | 80,229 | | | 71,476 | | |
| exceptions and tracebacks (exc.rs) | 36,272 | 42,622 | 448 | | 36,720 | | |
| match statement: compiler, runtime, parser patterns | 26,242 | 28,454 | 1,588 | | 27,830 | | |
| math and libm | 51,967 | 56,547 | | | 51,967 | | |
| complex and cmath | 31,341 | 33,837 | | | 31,341 | | |
| str methods and formatting (strmeth, percent, text) | 105,944 | 112,803 | 3,340 | | 109,284 | | |
| bytes and bytearray | 56,336 | 62,797 | | | 56,336 | | |
| host API (host.rs, facade host_defs) | 21,427 | 26,038 | 692 | | 22,119 | | |

**What depends on what.** A small "remove it" number does not make a
feature removable:

- **Generators and coroutines (about 18 KB).** The prelude uses `yield`
  12 times (iteration twins of builtins and coroutine plumbing) and
  `async def`/`await` 79 times; all of asyncio rests on coroutines. 15
  stdlib modules use `yield`: calendar, collections, collections.abc,
  contextlib, difflib, enum, heapq, pprint, re, statistics, string, types,
  typing, urllib.parse and uuid. Generator expressions need generators
  too (`compiler.rs::genexp`).
- **Classes and inheritance (about 70 KB for class.rs, 17 KB of it the
  inheritance machinery).** 36 of the 66 stdlib source files define subclasses,
  and the prelude has 28 classes, 11 with bases. User-defined exceptions
  subclass builtin ones. `super()` (3 KB) is used by abc, calendar,
  datetime, enum, html.parser, io, logging, random, re, typing and
  zoneinfo._zoneinfo. Descriptors (4 KB) are used by 33 modules and the
  prelude (8 decorator uses).
- **asyncio (about 106 KB).** It cannot be turned off: `mod-asyncio`
  gates no code. aio.rs, the `aio_*` builtins and the prelude's asyncio
  section are always built. collections.abc, contextlib and types define
  async APIs. Making it optional would need cfg-gating aio.rs, the
  builtins, the scheduler hooks in interp.rs and the prelude section (the
  prelude has no per-section gating today). Removable upper bound about
  100 KB [E].
- **Regex.** 15 modules import `re`: _markupbase, datetime._strptime,
  decimal, fnmatch, fractions, html, html.parser, logging, os.path, pprint,
  textwrap, tomllib._re, urllib.parse, warnings and zoneinfo._zoneinfo.
  The −44,745 removal leaves them importable but broken.
- **datetime.** zoneinfo, calendar, tomllib and `_strptime` depend on it.
  So does logging's time formatting, which uses `time`.
- **Big integers.** Unbounded `int` is language semantics. decimal,
  fractions, random (`getrandbits`), hashlib digests as ints and json all
  rely on it. Not removable.
- **Parser and compiler (about 227 KB of code, plus about 6.5 KB of their
  strings).** Any source needs them. So do `exec`/`eval`/`compile` and
  tracebacks' source carets. The stdlib and the prelude are compiled at
  build time and don't need them at run time. Only an embedding that
  restores snapshots compiled elsewhere could leave them out; the facade
  has no such mode today [E].
- **GC (about 16 KB).** It is needed to reclaim cycles and between cells,
  and to free code (`lifecycle.rs`). Not optional.
- **unicodedata (+376 KB).** It is opt-in and absent from the default
  build. `str` methods use core's own unicode tables (about 4.5 KB) and
  `printable.bin` (3 KB).

## Top 50 functions

In hello.wasm, LTO and wasm-opt fold the interpreter into `main`. The
guest and native builds show the interpreter on its own. [M]

| # | hello.wasm uu | bytes | native hello | bytes |
|---:|---|---:|---|---:|
| 1 | `hello::main` (the facade and the interpreter: `Session::start`, `execute`, `dispatch`, `dispatch_rare`) | 191,250 | `Vm::dispatch` | 36,951 |
| 2 | `Compiler::stmt_inner` | 24,217 | `Vm::dispatch_rare` | 32,580 |
| 3 | `Compiler::expr` | 15,881 | `Vm::read_snapshot` | 28,720 |
| 4 | `Vm::getattr` | 14,934 | `Vm::write_snapshot_to` | 22,130 |
| 5 | `Vm::percent` | 14,888 | `Compiler::stmt_inner` | 20,237 |
| 6 | `re::re_exec` | 14,731 | `Vm::execute` | 18,643 |
| 7 | `Vm::repr_walk` | 13,013 | `mm_parse::parse` | 15,280 |
| 8 | `Vm::walk_step` | 12,441 | `Vm::getattr` | 13,765 |
| 9 | `Vm::binop` | 12,396 | `Vm::collect_garbage` | 13,334 |
| 10 | `Vm::collect_garbage` | 11,801 | `hello::main` | 13,198 |
| 11 | `Vm::call_value` | 11,256 | `Session::start` | 12,610 |
| 12 | `Compiler::pattern` | 11,246 | `Vm::repr_walk` | 12,463 |
| 13 | `Parser::statement` | 11,193 | `Parser::statement` | 11,289 |
| 14 | `Vm::finish` | 9,629 | `Vm::walk_step` | 9,834 |
| 15 | `Vm::host_value` | 9,246 | `json::pl_json_encode` | 9,472 |
| 16 | `Vm::format` | 8,949 | `Parser::simple_stmts` | 9,306 |
| 17 | `json::pl_json_encode` | 8,869 | `Vm::host_value` | 8,857 |
| 18 | `Vm::find_module` | 8,660 | `Vm::finish` | 8,848 |
| 19 | `json::pl_json_scan` | 8,507 | `json::pl_json_scan` | 8,706 |
| 20 | `Parser::simple_stmts` | 7,833 | `Vm::pct_one` | 8,677 |
| 21 | `datetimemod::dt_strptime` | 6,966 | `Vm::call_value` | 8,590 |
| 22 | `Vm::drive` | 6,689 | `Vm::find_module` | 8,190 |
| 23 | `Vm::getitem` | 6,583 | `Vm::add_entry` | 7,675 |
| 24 | `Vm::final_line_with` | 6,508 | `re::re_exec` | 7,586 |
| 25 | `Vm::bind_args` | 6,270 | `Vm::binop` | 7,565 |
| 26 | `Vm::prelude_unit` | 6,064 | `Vm::percent` | 7,445 |
| 27 | core `dragon::format_shortest` | 6,045 | `Vm::drive` | 7,333 |
| 28 | `Parser::atom` | 5,951 | `Vm::bind_args` | 7,250 |
| 29 | `mmmod::mm_signature` | 5,923 | `re::Run::attempt` | 7,242 |
| 30 | `Vm::add_entry` | 5,711 | `Vm::format_frame` | 6,703 |
| 31 | `symtable::Builder::stmt` | 5,446 | `Vm::py_value_depth` | 6,101 |
| 32 | `Compiler::try_except` | 5,430 | `hashlib::St::compress` | 6,018 |
| 33 | `hashlib::St::compress` | 5,415 | `Compiler::try_except` | 5,987 |
| 34 | `Vm::iter_next` | 5,274 | `Compiler::expr` | 5,943 |
| 35 | `Vm::call_native` | 5,266 | `snapshot::read_host` | 5,794 |
| 36 | `Vm::py_value_depth` | 5,172 | `Heap::check_objects` | 5,741 |
| 37 | `Vm::format_frame` | 5,109 | `structmod::pack` | 5,649 |
| 38 | `Vm::new_class` | 4,996 | `Vm::new_class` | 5,610 |
| 39 | `Vm::finish_cell` | 4,970 | `Vm::cancel_tree` | 5,504 |
| 40 | `Vm::hash_depth` | 4,949 | `Compiler::call` | 5,486 |
| 41 | `Vm::setitem` | 4,894 | `Vm::iter_next` | 5,473 |
| 42 | `Vm::start_eq` | 4,884 | `Vm::call_native` | 5,395 |
| 43 | core `dragon::format_exact` | 4,850 | core `dragon::format_shortest` | 5,377 |
| 44 | `hashlib::St::digest` | 4,847 | `text::float_repr` | 5,359 |
| 45 | `text::float_repr` | 4,841 | `Vm::finish_cell` | 5,288 |
| 46 | dlmalloc `malloc` | 4,814 | `Parser::closed_pattern` | 5,230 |
| 47 | `structmod::pack` | 4,800 | `Vm::format` | 5,211 |
| 48 | `Vm::cancel_tree` | 4,700 | `Vm::new` | 5,183 |
| 49 | `mm_parse::walk::Walk::body` | 4,692 | `Vm::getitem` | 5,064 |
| 50 | `strmeth::format` | 4,521 | `Vm::setitem` | 4,944 |

Notes:
- None of these looks like bloat in the sense of dead or duplicated
  code. `main` in hello.wasm is the interpreter after inlining; no second
  copy of `execute` or `dispatch` exists.
- The snapshot pair (`read_snapshot` 28.7 KB, `write_snapshot_to` 22.1 KB
  natively) and `Heap::check_objects` (5.7 KB) are the persistence
  capability. They are large because each object kind has its own codec
  and its own validation. In the guest, `mm_session_restore` (45 KB) and
  `mm_snapshot` (19 KB) are those functions inlined into the exports.
- `percent` (14.9 KB in wasm, plus `pct_one` 8.7 KB natively) and `format`
  (8.9 KB) are `%` formatting and `format()`. Every conversion is inlined.
  At opt-level 3 these are among the larger cold-but-not-opt-in paths.
- core's `dragon::format_shortest` and `format_exact`, together with Grisu
  (about 17.5 KB of code and 3 KB of tables): `float_repr` uses `{:e}`
  and `%`/`format` use exact formatting. Both are needed; no cheaper exact
  formatter exists in core.
- `mm_signature` (5.9 KB) is `_mm.signature`. It builds a Python function
  with a given parameter list that binds arguments as CPython does. dataclasses
  uses it for the generated `__init__`.

## Monomorphization and duplication

twiggy `monos` and the script's own grouping. The script removes generic
arguments from names and counts the bytes beyond the largest copy [M]:

| generic family | copies wasm | bytes wasm | beyond largest | copies native | bytes native | beyond largest |
|---|---:|---:|---:|---:|---:|---:|
| `core::ptr::drop_glue` | 64 | 10,024 | 8,078 | 88 | 13,572 | 11,482 |
| `BTreeMap::insert` (mm-compile) | 2 | 3,619 | 1,801 | 2 | 4,042 | 2,011 |
| `Space::sweep` (two `Store` types) | 2 | 3,191 | 1,571 | 2 | 3,464 | 1,678 |
| `Space::place_slow` | (inlined) | | | 2 | 3,054 | 1,525 |
| `Filter::next` | 4 | 2,450 | 1,418 | 3 | 1,871 | 877 |
| btree `Handle::insert_fit` | 5 | 1,557 | 1,227 | 5 | 1,462 | 1,136 |
| `BTreeMap` `Drop` | 4 | 1,385 | 1,017 | 4 | 1,367 | 985 |
| `Vec::push` | 16 | 703 | 616 | 17 | 1,080 | 990 |
| `SpecFromIterNested::from_iter` | 3 | 967 | 371 | 11 | 3,837 | 3,274 |
| `RawVec::grow_one` | 30 | 442 | 352 | 58 | 2,971 | 2,885 |
| `String` `Extend` | 2 | 1,155 | 542 | 5 | 2,688 | 1,990 |
| wasm-opt outlined functions (`func[N]`) | 22 | 3,238 | | | | |

Across the 80 largest families, copies beyond the first add up to 26 KB
in hello.wasm, 35 KB in the guest and 43 KB natively [M]. Most generic code
is inlined instead and counts for the files that use it. std/core/alloc's
own out-of-line functions are 128 KB of hello.wasm.

No large function exists twice. Its main candidates were checked:
`execute`/`dispatch` in hello.wasm, and the guest exports against the
facade.

## Removable without losing capability or speed

These are candidates; the levers thread owns the engine changes.

| # | candidate | saving | evidence |
|---|---|---|---|
| 1 | `pub static NATIVES: &[Native] = &PACKED.natives;` (builtins.rs:1591) borrows a field of the `PACKED` const, so the whole compile-time `Packed` ships as one allocation. That includes `strs: [Sref; STR_CAP]` (25,672 B on wasm32), `blob` at its full `STR_CAP` and `lists` at its full capacity. Copy the field into its own static, as `LISTS`/`MODULES` already do. | native about −33 KB, hello.wasm and guest file about −5 KB, plus about 33 KB less static linear memory [E] | One 42,496 B anonymous constant at 0x124e9c in the wasm-ld map; that is exactly `size_of::<Packed>()` on wasm32 (582 × 16 + 6,418 + 596 + 256 + 25,672 + 226 + 16). Natively `size_of::<Packed>()` is 47,168, all of it in `.data.rel.ro` [M]. |
| 2 | `cargo xtask size` builds `hello` as an example, so the facade's dev-dependency `micro-monty = { features = ["test-hooks"] }` is unified into the measured build. | −312 wasm, −176 native [M]. A measurement fix; embedders are not affected. | removal build "facade without test-hooks" |
| 3 | `Space::sweep`/`place_slow` exist once per `Store` type. A non-generic core with the store as a parameter would merge them. | about −2 to −3 KB [E]. GC path, Ir check needed. | monos table |
| 4 | `BTreeMap` with several key types in mm-compile (also in the size-pass handoff). | about −2 to −4 KB [E] | monos table |
| 5 | `drop_glue` for `Session` (1.9 KB) and `Vm` (1.4 KB), and the 60 smaller ones: mostly unavoidable. Only the two `HostValue` copies (0.55 KB each) look like a mergeable pair. | about −0.5 KB [E] | top functions |

Levers that trade size against speed, or need an owner decision, are not
free:
- **Inlined accessors.** `Vm::slot`, `raw`, `hx`, `str_ref`, `set_raw`,
  `set_slot`, `int_of` and `builtins::arg` account for about 165 KB
  across their call sites in hello.wasm [M attribution]. An out-of-line
  slow path, or `#[optimize(size)]` on the cold breadth when Rust 1.101
  lands (handoff "Size by build structure": −112/−153 KB wasm, measured
  free on speed), is where most of this would come from. Making the
  accessors themselves out of line would cost dispatch speed.
- **mm-wasm-alloc call sites.** They take 15 KB of hello.wasm, about 10 B
  per site: the call sequence itself, and already the fix for
  `validate_size`. A smaller allocator (for example a bump/size-class
  allocator instead of dlmalloc's 7.5 KB of functions) would save at most
  about 7 KB [E] at a cost to fragmentation behaviour.
- **asyncio and json behind real features.** Up to about 100 KB and 28 KB
  [E] for builds without them. The owner's call against the codemode
  census (S1/T1).
- **Stdlib bytecode structure** (handoff "Size pass" §4): solid or
  chunked streams −29 to −107 KB, a stronger entropy coder up to about
  −43 KB, opt-in rarely used modules (decimal alone −39.9 KB [M]).
- **Prelude `dir` lists (10 KB) and GenericAlias/Union (8 KB).** The
  `dir` names could be generated from the native tables plus a shorter
  exception list [E, unmeasured].
- **Native PIE.** PIE needs the 38 KB of `.rela.dyn` that relocates the
  63 KB of `.data.rel.ro`. A non-PIE build is −64 KB (handoff "Size by
  build structure"). The owner decided to keep PIE.

## Caveats

- Per-file and capability numbers for wasm come from the module before
  wasm-opt, scaled by one global ratio [E]. Function-level numbers are
  exact after wasm-opt [M].
- Line tables attribute a byte to the line that generated it. Spill and
  setup code at a function's entry usually goes to the function's first
  line, which is harmless for per-file totals. About 0.3% of code has no
  line information ("no line info").
- Capability rows overlap: a byte in an aio.rs function inlined into
  `dispatch[op::AWAIT]` counts for both asyncio and generators.
- Natively, anonymous constants are not assigned to crates.
- Removal builds were taken for stdlib features only. Language features
  have no feature flags, so their numbers are attributions.
- mm-engine's `struct` feature cannot be turned off: builtins.rs gates
  `_unpack` but not `_calclen`, so `-struct` and a build with no stdlib at
  all fail to compile. The "all stdlib modules" removal therefore keeps
  struct and operator.
