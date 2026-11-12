#!/bin/sh
# Size builds of the two engines (README "Builds"). Outputs in $PENG/size/.
#   pocketpy-native: release pocketpy.c + hello_pocketpy.c, -Os -flto,
#     gc-sections, PK_ENABLE_OS=0, stripped.
#   pocketpy.wasm: the same for wasm32-wasip1 with wasi-sdk 34 (-Oz -flto),
#     then wasm-opt -Oz (binaryen 133), as `cargo xtask size` finishes mm's.
#   micropython-native: the sandbox variant at upstream's -Os, stripped
#     (micropython/build.sh); micropython-standard: the unix standard variant.
#   micropython.wasm: upstream's webassembly port as published on npm
#     (@micropython/micropython-webassembly-pyscript 1.29.0-6; emscripten -Os);
#     emscripten is not on the orb, so it is not rebuilt.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
PENG=${PENG:-/tmp/peng}
PK=$PENG/dl/pocketpy/amalgamated
WASI=$PENG/dl/wasi/wasi-sdk-34.0-x86_64-linux
OUT=$PENG/size
mkdir -p "$OUT"
FLAGS="-std=c11 -DNDEBUG -DPK_ENABLE_OS=0 -ffp-contract=off -ffunction-sections -fdata-sections"
gcc $FLAGS -Os -flto -I"$PK" -o "$OUT/pocketpy-native" "$PK/pocketpy.c" "$HERE/hello_pocketpy.c" -lm -Wl,--gc-sections -s
"$WASI/bin/clang" --target=wasm32-wasip1 --sysroot="$WASI/share/wasi-sysroot" $FLAGS -D_POSIX_C_SOURCE=200809L -Oz -flto -I"$PK" \
  -D_WASI_EMULATED_SIGNAL -D_WASI_EMULATED_PROCESS_CLOCKS -lwasi-emulated-signal -lwasi-emulated-process-clocks \
  -Wl,--gc-sections -Wl,--strip-all -o "$OUT/pocketpy-raw.wasm" "$PK/pocketpy.c" "$HERE/hello_pocketpy.c"
"${WASM_OPT:-$HOME/.local/bin/wasm-opt}" -Oz --enable-bulk-memory --enable-sign-ext --enable-mutable-globals \
  --enable-nontrapping-float-to-int "$OUT/pocketpy-raw.wasm" -o "$OUT/pocketpy.wasm"
cp "$PENG/bin/micropython-os" "$OUT/micropython-native"
strip -o "$OUT/micropython-standard" "$PENG/dl/micropython-1.29.0/ports/unix/build-standard/micropython"
cp "$PENG/dl/npm-mpy/package/micropython.wasm" "$OUT/micropython.wasm"
ls -l "$OUT"
