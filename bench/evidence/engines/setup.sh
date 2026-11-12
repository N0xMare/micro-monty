#!/usr/bin/env bash
# Fetches every engine at its pin (README "Pins") into $ENG and builds what
# run.py measures: mm (`cargo xtask size`, `mm`), the C `qjs` CLIs, the djs
# CLI and wasm module, quickjs-wasi's npm module, the hosts and the size
# hosts. Idempotent: a fetched pin or built CLI is kept (FORCE=1 rebuilds the
# CLIs); cargo rebuilds what changed. Logs go to $ENG/logs.
#
#   bench/evidence/engines/setup.sh
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
ENG=${ENG:-/tmp/eng}
# The hosts' manifests name /tmp/eng (djs, dquickts sources).
[ "$ENG" = /tmp/eng ] || echo "warning: hosts/*/Cargo.toml read djs and dquickts from /tmp/eng" >&2
FORCE=${FORCE:-0}
TOOLCHAIN=1.99.0
mkdir -p "$ENG"/{dl,bin,wasm,qwasi,logs}
log=$ENG/logs
step() { echo "== $*" >&2; }

QJS_NG_REV=d7215b0ad3fc2c562e8317fd9bca058fd4863804 # v0.18.0
DJS_REV=0e58f62c80252fc3f4156ab7a3744746434ed860
DQUICKTS_REV=dad0bafd09b15d298886d316161b2fe2e078e4aa
BELLARD=quickjs-2026-06-04
BELLARD_SHA=b376e839b322978313d929fd20663b11ba58b75df5a46c126dd19ea2fa70ad2a
QWASI_VERSION=3.6.2
QWASI_SHA=d4c9375f2b1ca4dc95f72c8aa2982a7a9951ac8011490d79c6582df732b4bbd9
WASM_FEATURES=(--enable-bulk-memory --enable-bulk-memory-opt --enable-call-indirect-overlong
  --enable-multivalue --enable-mutable-globals --enable-nontrapping-float-to-int
  --enable-reference-types --enable-sign-ext) # xtask/src/wasm.rs

checkout() { # DIR URL REV
  local dir=$ENG/$1
  [ -d "$dir/.git" ] || git clone -q --filter=blob:none "$2" "$dir"
  git -C "$dir" cat-file -e "$3^{commit}" 2>/dev/null || git -C "$dir" fetch -q origin
  git -C "$dir" checkout -q --detach "$3"
  [ "$(git -C "$dir" rev-parse HEAD)" = "$3" ] || { echo "$1: not at $3" >&2; exit 1; }
}
sha() { # FILE SHA256
  echo "$2  $1" | sha256sum -c --quiet - || { echo "$1: sha256 mismatch" >&2; exit 1; }
}
built() { [ "$FORCE" = 0 ] && [ -s "$1" ]; }
wasm_opt() { "${WASM_OPT:-$(command -v wasm-opt || echo "$HOME/.local/bin/wasm-opt")}" "$@"; }

step "sources"
checkout quickjs-ng https://github.com/quickjs-ng/quickjs.git $QJS_NG_REV
checkout djs https://github.com/earendil-works/djs.git $DJS_REV
checkout dquickts https://github.com/stencil-hq/dquickts.git $DQUICKTS_REV
if [ ! -s "$ENG/dl/$BELLARD.tar.xz" ]; then
  curl -sSfL -o "$ENG/dl/$BELLARD.tar.xz" "https://bellard.org/quickjs/$BELLARD.tar.xz"
fi
sha "$ENG/dl/$BELLARD.tar.xz" $BELLARD_SHA
if [ ! -s "$ENG/qwasi/quickjs.wasm" ]; then
  rm -rf "$ENG/dl/qwasi" && mkdir -p "$ENG/dl/qwasi"
  (cd "$ENG/dl/qwasi" && npm pack -q "quickjs-wasi@$QWASI_VERSION" >/dev/null &&
    tar xzf "quickjs-wasi-$QWASI_VERSION.tgz")
  cp "$ENG/dl/qwasi/package/quickjs.wasm" "$ENG/qwasi/quickjs.wasm"
fi
sha "$ENG/qwasi/quickjs.wasm" $QWASI_SHA

step "micro-monty: cargo xtask size, mm"
(cd "$root" && cargo xtask size && cargo build --release --locked -p mm-cli) >"$log/mm.log" 2>&1

step "quickjs-ng qjs"
if ! built "$ENG/bin/qjs-ng"; then
  command -v cmake >/dev/null || uv tool install -q cmake
  cmake -S "$ENG/quickjs-ng" -B "$ENG/qjs-ng-build" -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON >"$log/qjs-ng.log" 2>&1
  cmake --build "$ENG/qjs-ng-build" --target qjs_exe -j "$(nproc)" >>"$log/qjs-ng.log" 2>&1
  strip -o "$ENG/bin/qjs-ng" "$ENG/qjs-ng-build/qjs"
fi

step "QuickJS qjs"
if ! built "$ENG/bin/qjs-bellard"; then
  rm -rf "$ENG/dl/$BELLARD" && tar xJf "$ENG/dl/$BELLARD.tar.xz" -C "$ENG/dl"
  make -C "$ENG/dl/$BELLARD" -j "$(nproc)" CONFIG_LTO=y qjs >"$log/qjs-bellard.log" 2>&1
  strip -o "$ENG/bin/qjs-bellard" "$ENG/dl/$BELLARD/qjs"
fi

step "djs CLI and wasm module"
if ! built "$ENG/bin/djs"; then
  (cd "$ENG/djs" && RUSTUP_TOOLCHAIN=$TOOLCHAIN CARGO_TARGET_DIR="$ENG/djs-target" \
    cargo build --release --locked -p djs-cli) >"$log/djs-cli.log" 2>&1
  strip -o "$ENG/bin/djs" "$ENG/djs-target/release/djs"
fi
if ! built "$ENG/wasm/djs_wasm.wasm"; then
  (cd "$ENG/djs" && RUSTUP_TOOLCHAIN=$TOOLCHAIN scripts/build-wasm.sh) >"$log/djs-wasm.log" 2>&1
  wasm_opt -Oz "${WASM_FEATURES[@]}" "$ENG/djs/target/wasm32-unknown-unknown/wasm/djs_wasm.wasm" \
    -o "$ENG/wasm/djs_wasm.wasm"
fi

step "hosts"
for h in mm rquickjs dquickts djs mm-wasm quickjs-wasi; do
  case $h in mm-wasm | quickjs-wasi) t=$ENG/target-mm-wasm ;; *) t=$ENG/target-$h ;; esac
  (cd "$here/hosts/$h" && CARGO_TARGET_DIR=$t cargo build --release) >"$log/host-$h.log" 2>&1 ||
    { tail -20 "$log/host-$h.log" >&2; exit 1; }
done

step "size hosts"
"$here/size/build.sh" >"$log/size.log" 2>&1 || { tail -20 "$log/size.log" >&2; exit 1; }
step "ready"
