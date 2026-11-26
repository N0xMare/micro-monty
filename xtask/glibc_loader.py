"""Smoke the shipped mm at its GNU/Linux loader floor, without rebuilding it.

Run `python3 xtask/glibc_loader.py <shipped-mm>` on x86-64 or AArch64 Linux.
Cold runs need network access and dpkg-deb. Warm runs use the extracted
SHA-256-pinned libc6 packages under ${XDG_CACHE_HOME:-$HOME/.cache}/micro-monty.
"""

import hashlib
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
from urllib.request import urlopen


DEBIAN = "https://snapshot.debian.org/file/"
UBUNTU = "https://launchpadlibrarian.net/"
# Debian 12's libc6 2.36-9+deb12u14 and Ubuntu 22.04's 2.35-0ubuntu3.18.
# Use archival URLs so a newer security update need not invalidate the pins.
PACKAGES = {
    "x86_64": (
        "ld-linux-x86-64.so.2",
        (DEBIAN + "c534c6c7f912b61ca26b7cf4082235a0343b8751",
         "ba4f88f73dbc3ae9055f3c20f4523bfdbaf1ad13ff95e258924f77d20b4fbedf"),
        (UBUNTU + "883885933/libc6_2.35-0ubuntu3.18_amd64.deb",
         "cbed4dfa159019c759edae73ece1af67817e163686ad8925a26c90849ba1660a"),
    ),
    "aarch64": (
        "ld-linux-aarch64.so.1",
        (DEBIAN + "85af57bf1f9fa075163e51c99d9536154c21ab7e",
         "01f4330719fd4f65580e16ea5a0527f372fca750e8f588d26deaf09f2d3b1cf4"),
        (UBUNTU + "883884836/libc6_2.35-0ubuntu3.18_arm64.deb",
         "552390697756c005f8eff39b03392b2d8c6e148420c3e237d779fb44e54b5e72"),
    ),
}
CASE = Path(__file__).resolve().parents[1] / "conformance/own/613_iterator_membership_handoff.py"
EXPECTED = b"True [1]\nFalse [2]\n"


def sysroot(cache, url, digest):
    root = cache / digest
    if not root.is_dir():
        cache.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=cache) as tmp:
            tmp = Path(tmp)
            package = tmp / "libc6.deb"
            with urlopen(url, timeout=30) as response:
                data = response.read()
            if hashlib.sha256(data).hexdigest() != digest:
                raise SystemExit(f"libc6 SHA-256 mismatch: {url}")
            package.write_bytes(data)
            subprocess.run(["dpkg-deb", "-x", package, tmp / "root"], check=True)
            # Publish only the complete tree; a concurrent run may win the rename.
            try:
                (tmp / "root").rename(root)
            except OSError:
                if not root.is_dir():
                    raise
    return root


def main(mm):
    arch = platform.machine()
    if sys.platform != "linux" or arch not in PACKAGES:
        raise SystemExit("glibc loader smoke requires x86-64 or AArch64 Linux")
    loader, minimum, previous = PACKAGES[arch]
    cache = Path(os.getenv("XDG_CACHE_HOME") or Path.home() / ".cache")
    cache /= "micro-monty/glibc-loader"
    for version, package in (("2.36", minimum), ("2.35", previous)):
        lib = sysroot(cache, *package) / f"lib/{arch}-linux-gnu"
        if not (lib / "libc.so.6").is_file():
            raise SystemExit(f"missing pinned libc.so.6 in {lib}")
        # No inherited LD_* settings or host ld.so cache. The pinned libc
        # sits next to its loader and takes precedence over host libraries.
        result = subprocess.run(
            [lib / loader, "--inhibit-cache", "--library-path", lib, mm, "run", CASE],
            env={}, capture_output=True, timeout=15,
        )
        if (
            version == "2.36"
            and result.returncode == 0
            and result.stdout == EXPECTED
            and not result.stderr
        ):
            print(f"glibc {version}: {CASE.name} passed")
        elif (
            version == "2.35"
            and result.returncode == 1
            and not result.stdout
            and b"version `GLIBC_ABI_DT_RELR' not found" in result.stderr
        ):
            print(f"glibc {version}: refused mm with GLIBC_ABI_DT_RELR")
        else:
            raise SystemExit(
                f"unexpected glibc {version} result (exit {result.returncode}): "
                f"stdout={result.stdout!r}, stderr={result.stderr!r}"
            )


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} <shipped-mm>")
    main(Path(sys.argv[1]).resolve(strict=True))
