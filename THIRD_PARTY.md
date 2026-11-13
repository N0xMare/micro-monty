# Third-party material

## Monty

`conformance/monty/` contains test cases, and `conformance/monty-scripts/` the
datatest's CPython-side helper scripts, copied unmodified from pydantic/monty
(https://github.com/pydantic/monty), commit
`66fff6bfdfa65d3966633ebc9da77057d5d2b3e8`, under the MIT License:

```text
The MIT License (MIT)

Copyright (c) Pydantic Services Inc. 2026 to present

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## plakous-cell

`mm-cell-worker` depends on `plakous-cell` from plak
(https://github.com/N0xMare/plak), MIT, as a Cargo dependency. No source is
copied.

## Recorded tool results

`conformance/codemode/real/traces.tar.xz`, `traces-js-vs-python.tar.xz` and
`traces-js-vs-python-host1.tar.xz` hold
recorded agent sessions whose tool results include excerpts of plak (MIT), of
CPython 3.14's C headers (PSF License) and of Rust crates in a cargo registry
(each under its own license, mostly MIT or Apache-2.0), read by the agents
during review. They are test
fixtures, replayed as data; nothing is built from them. The traces are redacted
(local paths, user name and an environment listing; see that directory's
README) and excluded from published packages.

## fdlibm

`crates/mm-engine/src/libm.rs` ports functions from fdlibm (`e_log.c`,
`e_rem_pio2.c`, `s_atan.c`, `e_atan2.c`, `e_asin.c`, `e_acos.c`,
`s_log1p.c`, `s_expm1.c`, `e_log10.c`, `e_sinh.c`, `e_cosh.c`, `s_tanh.c`,
`s_asinh.c`, `e_acosh.c`, `e_atanh.c`, `s_erf.c`),
including their coefficient tables, and `cbrt` from FreeBSD msun's
`s_cbrt.c` (fdlibm as optimized by Bruce D. Evans, same notice), under this
notice:

```text
Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.

Developed at SunPro, a Sun Microsystems, Inc. business.
Permission to use, copy, modify, and distribute this
software is freely granted, provided that this notice
is preserved.
```

## CPython algorithms

`crates/mm-engine/src/libm.rs` and `math.rs` port algorithms from CPython's
`Modules/mathmodule.c` (Lanczos `gamma`/`lgamma`, `m_sinpi`, `fsum`,
`vector_norm`, the triple-length `sumprod` accumulator), `json.rs`
follows `Modules/_json.c` (scanner states and error messages),
`complex.rs` ports `Objects/complexobject.c` (`_Py_c_prod`, `_Py_c_quot`,
`_Py_c_pow`, parsing, `repr`) and the complex parts of
`Python/formatter_unicode.c`, `cmath.rs` ports `Modules/cmathmodule.c`
(the kernels and their C99 Annex G special value tables), and
`binascii.rs` and `crates/mm-stdlib/python/binascii.py` follow
`Modules/binascii.c` (scan rules and messages), and `structmod.rs` follows
`Modules/_struct.c` and `PyFloat_Pack2`/`PyFloat_Unpack2` (format grammar,
sizes, conversions and messages), `datetimemod.rs` ports the ISO 8601
parsers behind `fromisoformat` from `Modules/_datetimemodule.c` line by
line, and `exc.rs` ports `levenshtein_distance` from `Python/suggestions.c`
for "Did you mean" hints. The engine's prelude
(`crates/mm-engine/src/prelude.py`) follows `Objects/stringlib/unicode_format.h`
(`str.format`'s MarkupIterator), `Objects/genericaliasobject.c` and
`Objects/unionobject.c` (`list[int]`, `int | None`), and CPython 3.14's
`Lib/asyncio/` (locks, queues, `Barrier`, `Runner`, timeouts, task groups,
cancellation). CPython is
distributed under the PSF License Agreement
(https://docs.python.org/3/license.html); Copyright (c) 2001 Python Software
Foundation.

## CPython standard library modules

The bundled Python modules in `crates/mm-stdlib/python/` are adapted from
CPython 3.14's pure-Python standard library (`Lib/collections/__init__.py`,
`Lib/heapq.py`, `Lib/bisect.py`, `Lib/functools.py`, `Lib/copy.py`,
`Lib/dataclasses.py`, `Lib/typing.py`, `Lib/operator.py`, `Lib/abc.py`,
`Lib/_py_abc.py`, `Lib/_collections_abc.py`, `Lib/enum.py`,
`Lib/string/__init__.py`, `Lib/textwrap.py`,
`Lib/random.py`, `Lib/hashlib.py`, `Lib/base64.py`, `Lib/_pydatetime.py`,
`Lib/_strptime.py`, `Lib/posixpath.py`, `Lib/genericpath.py`, `Lib/fnmatch.py`,
`Lib/csv.py`, `Lib/fractions.py`, `Lib/statistics.py`, `Lib/uuid.py`,
`Lib/types.py`, `Lib/_pydecimal.py` (as `decimal.py`), `Lib/numbers.py`,
`Lib/contextlib.py`, `Lib/traceback.py`, `Lib/logging/__init__.py` (as
`logging.py`), `Lib/urllib/parse.py`, `Lib/pprint.py`, `Lib/shlex.py`,
`Lib/difflib.py`, `Lib/html/__init__.py`, `Lib/html/entities.py`,
`Lib/html/parser.py`, `Lib/_markupbase.py`, `Lib/calendar.py`, `Lib/hmac.py`,
`Lib/zoneinfo/` (`__init__.py`, `_common.py`, `_zoneinfo.py`, `_tzpath.py`),
`Lib/_py_warnings.py` (as `warnings.py`), `Lib/ast.py`, `Lib/token.py`,
`Lib/tokenize.py`, `Lib/xml/etree/ElementTree.py` and
`Lib/xml/etree/ElementPath.py` (also under the ElementTree toolkit's
notice, Copyright (c) 1999-2008 by Fredrik Lundh, kept in each file; the
parser is micro-monty's), `Lib/email/utils.py` and `Lib/email/_parseaddr.py`
(the date functions),
`Lib/tomllib/` (also MIT, Copyright (c) 2021 Taneli
Hukkinen; relative imports made absolute);
`csv.py` also ports `Modules/_csv.c`'s reader and writer), and
`itertools.py` follows the pure-Python equivalents in CPython's `itertools`
documentation and the argument handling of `Modules/itertoolsmodule.c`.
Each file is modified (no `exec`/`eval`, no `re`, no `__slots__` or
`__prepare__`, C-type names and messages kept).
`time.py` follows `Modules/timemodule.c` (checks and messages),
`codecs.py` follows `Lib/codecs.py` and `Python/codecs.c`, and
`platform.py`, `io.py`, `contextvars.py`, `hashlib.py`, `struct.py`,
`zlib.py`, `os/__init__.py` and `pathlib.py` are written for micro-monty
after the CPython modules' API, argument handling and messages.
The class-annotation
stringizer in `crates/mm-parse/src/unparse.rs` follows
`Python/ast_unparse.c`. CPython is distributed under the PSF License
Agreement (https://docs.python.org/3/license.html); Copyright (c)
2001 Python Software Foundation; All Rights Reserved.

## Unicode Character Database

`crates/mm-engine/src/ucd/` holds tables generated by
`crates/mm-engine/tools/gen_ucd.py` from CPython 3.14's `unicodedata`
(Unicode 16.0.0); `crates/mm-engine/src/unicodedata.rs` follows CPython's
`Modules/unicodedata.c` (PSF License, as above) for its functions and
messages. The data is Unicode, Inc.'s, under the Unicode License v3
(https://www.unicode.org/license.txt): Copyright © 1991-2024 Unicode, Inc.

## CPython `re`

`crates/mm-stdlib/python/re.py` ports the pattern and replacement-template
parsers of CPython 3.14's `Lib/re/_parser.py` and the module surface of
`Lib/re/__init__.py` (Secret Labs' Regular Expression Engine, Copyright (c)
1998-2001 by Secret Labs AB, distributed by the PSF under the PSF License
Agreement). The matcher in `crates/mm-engine/src/re.rs` is micro-monty's
own; its repeat, assertion and `must_advance` semantics follow
`Modules/_sre/sre_lib.h` (same license).

## Mersenne Twister

`crates/mm-engine/src/random.rs` follows MT19937 as CPython's
`Modules/_randommodule.c` runs it (`init_genrand`, `init_by_array`,
`genrand_uint32`, `genrand_res53`), which carries this notice:

```text
Copyright (C) 1997 - 2002, Makoto Matsumoto and Takuji Nishimura,
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

  1. Redistributions of source code must retain the above copyright
     notice, this list of conditions and the following disclaimer.

  2. Redistributions in binary form must reproduce the above copyright
     notice, this list of conditions and the following disclaimer in the
     documentation and/or other materials provided with the distribution.

  3. The names of its contributors may not be used to endorse or promote
     products derived from this software without specific prior written
     permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE COPYRIGHT OWNER OR
CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```
