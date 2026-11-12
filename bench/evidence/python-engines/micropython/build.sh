#!/bin/sh
# Builds MicroPython's unix port in the sandbox variant (variant-sandbox/):
#   build-sandbox     -O2: the CLI and libmicropython.a the host links (speed)
#   build-sandbox-os  upstream's -Os: the size measurement
# MPY = the micropython-1.29.0 release tree, MPY_LIB_DIR = micropython-lib.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
PENG=${PENG:-/tmp/peng}
MPY=${MPY:-$PENG/dl/micropython-1.29.0}
LIB=${MPY_LIB_DIR:-$PENG/dl/micropython-lib}
rm -rf "$MPY/ports/unix/variants/sandbox"
cp -r "$HERE/variant-sandbox" "$MPY/ports/unix/variants/sandbox"
make -C "$MPY/mpy-cross" -j16 >/dev/null
for cfg in "build-sandbox -O2" "build-sandbox-os -Os"; do
  set -- $cfg
  make -C "$MPY/ports/unix" -j16 VARIANT=sandbox BUILD="$1" COPT="$2 -DNDEBUG" MPY_LIB_DIR="$LIB" >/dev/null
done
make -C "$MPY/ports/unix" -j16 VARIANT=sandbox BUILD=build-sandbox COPT="-O2 -DNDEBUG" MPY_LIB_DIR="$LIB" lib >/dev/null
mkdir -p "$PENG/bin"
cp "$MPY/ports/unix/build-sandbox/micropython" "$PENG/bin/micropython-bin"
strip -o "$PENG/bin/micropython-os" "$MPY/ports/unix/build-sandbox-os/micropython"
# The CLI with a 256 MiB GC heap (mm's default memory limit; the unix
# default is 2 MiB).
printf '#!/bin/sh\nexec %s -X heapsize=256M "$@"\n' "$PENG/bin/micropython-bin" > "$PENG/bin/micropython"
chmod +x "$PENG/bin/micropython"
# The host links the library without the CLI's main.
cp "$MPY/ports/unix/build-sandbox/libmicropython.a" "$PENG/bin/libmicropython.a"
ar d "$PENG/bin/libmicropython.a" main.o
# The compiler flags of the port's own objects, with absolute include paths,
# for the host's shim.c (hosts/micropython/build.rs).
U="$MPY/ports/unix"
touch "$U/main.c"
make -n -C "$U" VARIANT=sandbox BUILD=build-sandbox COPT="-O2 -DNDEBUG" MPY_LIB_DIR="$LIB" build-sandbox/main.o 2>/dev/null \
  | grep '^gcc.*-o build-sandbox/main.o' | head -1 | sed -e 's/ -c -MD -MF.*//' -e 's/^gcc //' -e 's/-U _FORTIFY_SOURCE/-U_FORTIFY_SOURCE/' \
  | tr ' ' '\n' | grep -v -e '^-W' -e '^$' -e '\\"' \
  | sed -e "s|^-I\.\./\.\./|-I$MPY/|" -e "s|^-I\.\./\.\.$|-I$MPY|" -e "s|^-I\.$|-I$U|" -e "s|^-I\([a-z]\)|-I$U/\1|" \
  > "$PENG/bin/mpy-cflags.txt"
