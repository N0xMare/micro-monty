"""zstd-1 with a trained dictionary, against FULL+zstd1 and the base as prefix.

    python3.14 -I bench/evidence/base_relative/dictionary.py KEEP_DIR BASE.mmsnap

KEEP_DIR is run.py's `--keep` output (one directory per run, `<park>.full`
inside). The dictionary is trained on the codemode parks of sets `a`,
`luna-a` and `sol-a` (at most about 1,500 of them, evenly spaced) and tested on the other sets' parks (held out by set,
so one task's solutions appear on both sides but no solution does) and on
the own cases. Sizes only; [M].
"""
import statistics
import sys
from pathlib import Path

from compression import zstd

keep, base = Path(sys.argv[1]), Path(sys.argv[2]).read_bytes()
TRAIN = ("codemode__a__", "codemode__luna-a__", "codemode__sol-a__")
runs = sorted(p for p in keep.iterdir() if p.is_dir())


def images(pred, first_only=False):
    out = []
    for run in filter(lambda r: pred(r.name), runs):
        parks = sorted(run.glob("*.full"), key=lambda p: int(p.stem))
        out += [p.read_bytes() for p in parks[:1 if first_only else None]]
    return out


train = images(lambda n: n.startswith(TRAIN))
train = train[::max(1, len(train) // 1500)]  # zstd's trainer wants a bounded sample
tests = {
    "codemode held-out sets, first park": images(
        lambda n: n.startswith("codemode__") and not n.startswith(TRAIN), True),
    "codemode held-out sets, every park": images(
        lambda n: n.startswith("codemode__") and not n.startswith(TRAIN)),
    "own cases, first park": images(lambda n: n.startswith("own__"), True),
}
prefix = zstd.ZstdDict(base, is_raw=True).as_prefix
print(f"training images: {len(train)}, base {len(base):,} B\n")
print("| Images | n | FULL | FULL+zstd1 | base prefix | dict 64 KiB | dict 112 KiB | base + dict 112 KiB (prefix) |")
print("|---|---:|---:|---:|---:|---:|---:|---:|")
dicts = {k: zstd.train_dict(train, k << 10) for k in (64, 112)}
# The base and a trained dictionary as one raw prefix.
both = zstd.ZstdDict(base + dicts[112].dict_content, is_raw=True).as_prefix
for name, imgs in tests.items():
    def ratio(f):
        return statistics.median(len(i) / len(f(i)) for i in imgs)
    med = statistics.median(len(i) for i in imgs)
    cols = [ratio(lambda i: zstd.compress(i, 1)),
            ratio(lambda i: zstd.compress(i, 1, zstd_dict=prefix))]
    cols += [ratio(lambda i, d=d: zstd.compress(i, 1, zstd_dict=d)) for d in dicts.values()]
    cols.append(ratio(lambda i: zstd.compress(i, 1, zstd_dict=both)))
    for d in dicts.values():  # round trip
        assert zstd.decompress(zstd.compress(imgs[0], 1, zstd_dict=d), zstd_dict=d) == imgs[0]
    print(f"| {name} | {len(imgs)} | {med:,.0f} | " + " | ".join(f"{c:.1f}×" for c in cols) + " |")
