#!/bin/sh
# Rebuilds every engine and host of the Python-engine comparison and reruns
# it (docs/research/python-engines-compare.md; README "Rerunning").
#
#   bench/evidence/python-engines/rerun.sh [run.py options, e.g. --rounds 5]
#
# On a release candidate only mm changes (the other engines are pinned), so
#   rerun.sh --engines mm --reuse results/<earlier date> --out results/<date>
# reruns mm alone and copies the other engines' records from an earlier run
# on the same machine (about 10 minutes with warm builds, instead of 85).
#
# Sources go to $PENG/dl (default /tmp/peng), engine binaries to $PENG/bin,
# size builds to $PENG/size, Rust hosts to $ENG/target-* (default /tmp/eng).
# Needs: gcc, make, cargo, curl, npm, git, python3.14, wasm-opt (binaryen
# 133), as on an orb set up by .agents/setup.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
export PENG=${PENG:-/tmp/peng} ENG=${ENG:-/tmp/eng}
DL=$PENG/dl
mkdir -p "$DL" "$PENG/bin"

# Pins (README "Pins").
MPY_VER=1.29.0
MPY_LIB_REV=660ddd89dd958d8d835ecbbcb323f3cfcd891c30
PK_TAG=v2.2.0
WASI=wasi-sdk-34
MPY_NPM=@micropython/micropython-webassembly-pyscript@1.29.0-6

cd "$DL"
[ -d micropython-$MPY_VER ] || { curl -sLO https://github.com/micropython/micropython/releases/download/v$MPY_VER/micropython-$MPY_VER.tar.xz; tar xf micropython-$MPY_VER.tar.xz; }
[ -d micropython-lib ] || { git clone -q https://github.com/micropython/micropython-lib.git; git -C micropython-lib checkout -q $MPY_LIB_REV; }
[ -d pocketpy ] || git clone -q --depth 1 --branch $PK_TAG https://github.com/pocketpy/pocketpy.git
[ -d wasi/$WASI.0-x86_64-linux ] || { mkdir -p wasi; curl -sL https://github.com/WebAssembly/wasi-sdk/releases/download/$WASI/$WASI.0-x86_64-linux.tar.gz | tar xz -C wasi; }
[ -f npm-mpy/package/micropython.wasm ] || { mkdir -p npm-mpy; (cd npm-mpy && npm pack -q $MPY_NPM >/dev/null && tar xzf ./*.tgz); }

# micropython-lib's pure-Python stdlib on MICROPYPATH for the CLI runs
# (capability.py); the host has the frozen set of micropython/variant-sandbox.
rm -rf "$PENG/mpylib"; mkdir -p "$PENG/mpylib"
for d in "$DL"/micropython-lib/python-stdlib/*/; do
  case $(basename "$d") in unittest*|venv|tarfile*|zipfile|ssl|pkg_resources|pkgutil) continue;; esac
  (cd "$d" && find . -name '*.py' ! -name manifest.py ! -name 'test_*.py' ! -path './test*' ! -path './examples/*' \
    -exec sh -c 'mkdir -p "$0/$(dirname "$1")" && cp "$1" "$0/$1"' "$PENG/mpylib" {} \;)
done
cp "$DL/micropython-lib/micropython/ucontextlib/ucontextlib.py" "$PENG/mpylib/"

# MicroPython: the unix standard variant (size reference), then the sandbox
# variant (CLI, host library, -Os size build).
make -C "$DL/micropython-$MPY_VER/mpy-cross" -j16 >/dev/null
make -C "$DL/micropython-$MPY_VER/ports/unix" -j16 VARIANT=standard >/dev/null
MPY="$DL/micropython-$MPY_VER" MPY_LIB_DIR="$DL/micropython-lib" "$HERE/micropython/build.sh"

# PocketPy: the amalgamated release files, and its CLI at upstream's release
# flags (CMakeLists: -O2, NDEBUG, -ffp-contract=off; OS module on).
(cd "$DL/pocketpy" && python3 prebuild.py && python3 amalgamate.py >/dev/null)
(cd "$DL/pocketpy" && gcc -std=c11 -O2 -DNDEBUG -ffp-contract=off -Iinclude -o "$PENG/bin/pocketpy" \
  $(find src -name '*.c') src2/main.c -lm -ldl -lpthread)

"$HERE/size/build.sh"

# mm: the shipped size builds, the CLI, the codemode runner, the hosts.
(cd "$ROOT" && cargo xtask size && cargo build --release --locked -p mm-cli)
(cd "$ROOT/bench/evidence/engines/hosts/mm" && CARGO_TARGET_DIR="$ENG/target-mm" cargo build --release -q)
for h in micropython pocketpy monty; do
  (cd "$HERE/hosts/$h" && CARGO_TARGET_DIR="$ENG/target-$h" cargo build --release -q)
done
# mm's codemode outcomes are conformance/codemode/results/runs.jsonl; check
# they still hold on this build before capability.py reads them.
(cd "$ROOT" && "${PYTHON:-$HOME/.local/bin/python3.14}" conformance/codemode/run.py --check)

cd "$HERE"
exec "${PYTHON:-$HOME/.local/bin/python3.14}" run.py "$@"
