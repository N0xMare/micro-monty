# dquickts comparator preparation

Prepared in the orb on 2026-11-04. These are checksum and API observations,
**not comparative timings**. Other builds were running concurrently.
No comparator engine source or benchmark workload was patched.

## Pinned source and local layout

The live remote's default branch was `master`.

| Configuration | Revision | Detached checkout / worktree |
|---|---|---|
| Normal JS | `b51e92c0c7ca3b0ae7afff1a1f2de8baf2e7a34a` | `/tmp/mm-evidence-dquickts/js` |
| Pure Python | `68be18f241e2c92a9a54fbcc3e6ceb3ad4914955` (`feat/py`) | `/tmp/mm-evidence-dquickts/py` |

The Python worktree shares the JS clone's Git object store. Keep both directories.
Both branches report QuickJS `2026-06-04`; `qpy` reports Python `3.15.2`.
Use revisions, not those version banners, to identify these builds.

```sh
mkdir -p /tmp/mm-evidence-dquickts
git clone --depth 1 --filter=blob:none --no-tags --branch master \
  https://github.com/stencil-hq/dquickts.git /tmp/mm-evidence-dquickts/js
git -C /tmp/mm-evidence-dquickts/js fetch --depth 1 origin \
  feat/py:refs/remotes/origin/feat/py
git -C /tmp/mm-evidence-dquickts/js switch --detach \
  b51e92c0c7ca3b0ae7afff1a1f2de8baf2e7a34a
git -C /tmp/mm-evidence-dquickts/js worktree add --detach \
  /tmp/mm-evidence-dquickts/py 68be18f241e2c92a9a54fbcc3e6ceb3ad4914955
```

## Builds

Linux x86-64, GNU Make 4.3. The normal JS build used GCC
`12.2.0-14+deb12u1` and the branch's default flags: `-O2 -g`, no LTO,
Intl/Temporal compiled in, no Python. The generated CLI and static library are
`js/qjs` and `js/libquickjs.a` under the preparation directory.

```sh
make -C /tmp/mm-evidence-dquickts/js -j2 qjs libquickjs.a
```

The Python build used Debian Clang `14.0.6-12`, `-O2 -g`, no LTO,
Intl/Temporal compiled in, `CONFIG_PYTHON=y`, **`CONFIG_PYTHON_CAPI=`**,
and **`CONFIG_QPY_NO_BASE=y`**. Thus it includes the Python runtime and vendored
pure-Python library, but no CPython C API extension modules, embedded startup
snapshot, or embedded frozen-module blob. No pip/native packages were installed.
The generated CLI and library are `py/qpy` and `py/libquickjs.a`.

Build deviations, all external configuration rather than engine patches:

1. The initial plain GCC pure-Python build failed: missing `INT_MAX` declarations,
   an implicit `isalnum` declaration, and nonconstant compound-literal static
   initializers in `python/mod_faulthandler.c`.
2. Clang was absent. Debian's `clang-14`, `libclang-cpp14`, `libllvm14`,
   `libclang-common-14-dev`, and `libclang1-14` packages were downloaded with
   `apt-get download` and extracted with `dpkg-deb -x` into
   `/tmp/mm-evidence-dquickts/toolchain/root`. Nothing was installed system-wide.
   Clang plus `-include limits.h -include ctype.h` compiled the untouched source.
3. The default startup snapshot generation then failed with
   `TypeError: snapshot: unregistered native code pointer` (followed by a
   process-specific address). The Makefile's documented `CONFIG_QPY_NO_BASE=y`
   option produced a working CLI. Do not compare its startup against claims for
   the default snapshot-enabled `qpy` configuration.

From `/tmp/mm-evidence-dquickts`, the successful Python command was:

```sh
export LD_LIBRARY_PATH=/tmp/mm-evidence-dquickts/toolchain/root/usr/lib/x86_64-linux-gnu
compiler='/tmp/mm-evidence-dquickts/toolchain/root/usr/bin/clang-14 -include limits.h -include ctype.h'
make -C py -j2 OBJDIR=.obj-pure-clang CONFIG_CLANG=y \
  CC="$compiler" HOST_CC="$compiler" CONFIG_PYTHON=y CONFIG_PYTHON_CAPI= \
  CONFIG_QPY_NO_BASE=y qpy libquickjs.a
```

`LD_LIBRARY_PATH` is needed for the locally extracted compiler, not for `qpy`.
The builds ran sequentially with at most two make jobs, not unbounded `-j`.
Preserved logs: `build-js.log`, `build-py.log` (GCC failure),
`build-py-clang.log` (startup-snapshot failure), `build-py-nobase.log`.
The Python checkout has an untracked build directory, but no tracked source diff.

## Run the existing workloads unchanged

No CLI adapter is needed. From the micro-monty repository:

```sh
/tmp/mm-evidence-dquickts/js/qjs /home/user/workspace/repo/bench/loop.js
env -u PYTHONPATH QPYHOME=/tmp/mm-evidence-dquickts/py \
  PYTHONHASHSEED=0 PYTHONDONTWRITEBYTECODE=1 \
  /tmp/mm-evidence-dquickts/py/qpy -B /home/user/workspace/repo/bench/loop.py
```

`-B` avoids writing bytecode caches alongside read-only workloads. Neither command
adds stdout framing or rewrites the checksum. Each is a fresh-process CLI boundary
(startup, source compilation, execution, and exit), not a warm in-process loop.

Executed once per workload, matching exact stdout bytes including the newline:

- [M] JS: **13/13** checksum matches; every exit status zero and stderr empty.
- [M] Pure Python: **12/13** matches; all except `json_rows.py` exited zero with
  empty stderr. JSON exited one with empty stdout because `json -> re -> _sre`
  raised `ModuleNotFoundError: Standard library module '_sre' was not found`.
  This is an unsupported workload in this configuration, not a substituted JSON
  implementation or a passing result. A C-API-enabled build was not attempted.

The external rerunner `/tmp/mm-evidence-dquickts/smoke.py` uses only the host
Python subprocess API; it does not run guest code on CPython. Run
`python3 /tmp/mm-evidence-dquickts/smoke.py js py`. It deliberately returns failure
for the unsupported Python JSON case. Raw stdout/stderr and SHA-256 summaries
are in `smoke/{js,py}/`, with the top-level `smoke-{js,py}.log` transcripts.
`workloads.sha256` records the workload and expected-file hashes; all were
verified unchanged after the smoke runs.

## Public-API suspension probe

`api_probe.c` is an external host adapter using **only `quickjs.h` APIs**.
It does not include internal Python headers, change frame flags, patch the VM,
modify existing workloads, or measure elapsed time. It is not a warm benchmark
driver. Build from `/tmp/mm-evidence-dquickts`:

```sh
gcc -std=gnu11 -O2 -g -Wall -Wextra -Werror -Wno-unused-parameter -I py \
  -c /home/user/workspace/repo/bench/evidence/dquickts/api_probe.c -o api_probe.o
gcc -o api_probe api_probe.o py/libquickjs.a -lm -lpthread -ldl -lutil
for entry in js-eval py-eval py-call js-to-py; do
  for operation in loop gate; do
    /tmp/mm-evidence-dquickts/api_probe /tmp/mm-evidence-dquickts/py \
      "$entry-$operation"
  done
done
```

Each invocation creates/frees a runtime and a context. Python initialization,
function definition and lookup happen before the request handler is installed.
`py-eval` enters statements with `JS_EvalPython(..., JS_PY_EVAL_EXEC)`;
`py-call` retrieves a Python function with `JS_EvalPython(..., EVAL)` then enters
it with public `JS_Call`; `js-to-py` installs that same function as a JS global
and enters `py_entry()` with `JS_Eval`. `js-eval` is the JS-only control using
the same Python-branch library.

The loop handler requests suspension only at its first invocation. The gate
is a `JS_NewCFunction` host callback returning `JS_SuspendCall(ctx)` directly;
Python reaches it with `from js import host_gate`. The host supplies `41` with
`JS_ResumeWith` and verifies the remaining `+1` through the printed completion.
No async tasks or promises implement this gate.

Executed observations [M], with all process exits zero and all probe stderr empty:

| Entry | Loop | Host gate |
|---|---|---|
| `js-eval` | suspended at first poll; resumed to `4999950000` | suspended; resumed to `42` |
| `py-eval` | ten polls; no suspension; completed to `4999950000` | not suspended; `TypeError: this call cannot be suspended` |
| `py-call` | suspended at first poll; resumed to `4999950000` | suspended; resumed to `42` |
| `js-to-py` | suspended at first poll; resumed to `4999950000` | suspended; resumed to `42` |

Supported suspension cases returned `JS_EXCEPTION` with both
`JS_IsSuspended(rt)` and `JS_IsSuspendException(ctx)` true. Resume then returned
normally with `JS_IsSuspended(rt)` false. Gate callback count remained one
after resume. Raw transcripts are `probes/<mode>.txt` and `.stderr`.
Guest exceptions intentionally remain diagnostic observations, not a nonzero
probe exit; inspect the transcript rather than assuming exit zero means support.

Static explanation [S], separate from the executed observations:

- [`JS_EvalPython` / `qp_jsi_run_main`](https://github.com/stencil-hq/dquickts/blob/68be18f241e2c92a9a54fbcc3e6ceb3ad4914955/python/jsinterop.c#L1965-L2027)
  calls [`qp_run_code`](https://github.com/stencil-hq/dquickts/blob/68be18f241e2c92a9a54fbcc3e6ceb3ad4914955/python/pythonrun.c#L49-L77),
  which reaches [`qp_call_with_locals`](https://github.com/stencil-hq/dquickts/blob/68be18f241e2c92a9a54fbcc3e6ceb3ad4914955/python/ceval.c#L1015-L1020)
  without the host-entry flag.
- The [`frame-entry/suspendability gate`](https://github.com/stencil-hq/dquickts/blob/68be18f241e2c92a9a54fbcc3e6ceb3ad4914955/quickjs.c#L21250-L21290)
  requires an outer host entry and inner direct bytecode entries. It does not
  reject normal Python bytecode merely for being Python.
- The [`JS_SuspendCall` check](https://github.com/stencil-hq/dquickts/blob/68be18f241e2c92a9a54fbcc3e6ceb3ad4914955/quickjs.c#L25844-L25854)
  raises the observed TypeError when the caller stack fails that gate.

These observations are **entry-path dependent**, not evidence that Python
suspension is universally absent or that all Python operations pause anywhere.
Only these small normal-function loops and direct host calls were exercised.
No snapshot/restore, cross-process durability, native-operation latency bound,
generator/async callback suspension, native extension, or package compatibility
is established by this probe. In particular, the failed startup-snapshot build
is not evidence that every Python heap snapshot fails.

## SHA-256 identity

Paths below are relative to `/tmp/mm-evidence-dquickts`, except the adapter source.
These identify local unstripped builds, not release-size or reproducible-build claims.

```text
8ff96ce8ca17463507bee2a7446bc1d978b81fce829700c96da17d0612c45c02  js/qjs
edae86bc50afaf6958a3c63fea1c3ca357f8640a371f031525792f5edd5063b5  js/libquickjs.a
59ecbe59036b9673a16e4dcaced3d5a8a88156fc23dcbe2f796a1b745a8d266f  py/qpy
af4e8cfd64d1b01f102165686d9bde8a63c2a528f3a4349ca802abc83a349868  py/libquickjs.a
a55b5f0f91567ef62a70115ba00fbf361d02bd83cf160499e71b0742589a1692  api_probe
ea2168dbe7c9f089b68801d67e04e867d9d0f65330e4cd795ac11d8636088c51  bench/evidence/dquickts/api_probe.c
```

`executables.sha256` preserves this inventory locally. Toolchain package hashes
are in `toolchain/packages.sha256`. No timing results, commits, or pushes were made.
