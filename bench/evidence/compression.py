"""Host compression of FULL snapshots: ratio and CPU cost per codec.

Usage: python3.14 bench/evidence/compression.py MM OUT.jsonl
Images: the first park of every conformance/own case (`mm exec
--snapshot-every 100`), plus retained-state programs of 1k/10k/100k rows
parked after building their data. Codecs are independent streams with no
dictionary; times are single-threaded, in-process, best of 5. Not timed:
storage, fsync or the engine's own snapshot/restore.
"""
import json
import subprocess
import sys
import tempfile
import time
import zlib
from pathlib import Path

from compression import zstd

ROOT = Path(__file__).resolve().parents[2]
CODECS = {
    "zlib-1": (lambda b: zlib.compress(b, 1), zlib.decompress),
    "zlib-6": (lambda b: zlib.compress(b, 6), zlib.decompress),
    "zstd-1": (lambda b: zstd.compress(b, 1), zstd.decompress),
    "zstd-3": (lambda b: zstd.compress(b, 3), zstd.decompress),
}
STATE = """data = [{{'i': i, 's': 'tag', 'f': i * 0.5, 'k': str(i)}} for i in range({rows})]
total = 0
while True:
    total += 1
"""


def park(mm, source, every):
    with tempfile.TemporaryDirectory() as directory:
        subprocess.run([mm, "exec", "--json", "--snapshot-every", str(every),
                        "--snapshot-dir", directory, source],
                       capture_output=True, timeout=60, check=False)
        image = Path(directory, "session.mmsnap")
        return image.read_bytes() if image.is_file() else None


def best(f, data):
    times = []
    for _ in range(5):
        start = time.perf_counter_ns()
        out = f(data)
        times.append(time.perf_counter_ns() - start)
    return out, min(times)


def main():
    mm, out = sys.argv[1], Path(sys.argv[2])
    images = []
    for case in sorted((ROOT / "conformance/own").glob("*.py")):
        image = park(mm, case, 100)
        if image:
            images.append((f"own/{case.name}", image))
    with tempfile.TemporaryDirectory() as directory:
        for rows in [1000, 10000, 100000]:
            source = Path(directory, f"state{rows}.py")
            source.write_text(STATE.format(rows=rows))
            image = park(mm, source, 20_000_000)
            assert image, f"state {rows} did not park"
            images.append((f"state/{rows}", image))
    with out.open("w") as output:
        for name, image in images:
            row = dict(image=name, raw_bytes=len(image))
            for codec, (encode, decode) in CODECS.items():
                packed, encode_ns = best(encode, image)
                unpacked, decode_ns = best(decode, packed)
                assert unpacked == image, (name, codec)
                row[codec] = dict(bytes=len(packed), encode_ns=encode_ns, decode_ns=decode_ns)
            output.write(json.dumps(row) + "\n")
    print(f"{len(images)} images written to {out}")


if __name__ == "__main__":
    main()
