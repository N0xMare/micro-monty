# A call's `**` spread of 200,000 keys grows the value stack (3.2 MB) for
# its arguments; then memory is filled to leave room for one more spread
# call of 300,000 keys. The session that goes on keeps the stack's
# capacity charged and a restored one charges what the image holds, so
# the second call raises in the first and completes in the second
# (restore-vs-snapshot, step 4). Which it did stays in a global, for the
# final-state digest: CPython completes it.
import gc
N = 200_000
base = {'k%d' % i: i for i in range(N)}
other = {'j%d' % i: i for i in range(N // 2)}


def f(**kw):
    return len(kw)


# Work that costs fuel but holds little memory: most parks fall here, with
# a small heap, and few after the fill, whose images are near the limit.
zeros = 0
block = bytes(1 << 20)
for _ in range(80):
    zeros += block.count(0)
block = None

# Fill up to 320 MiB, or the 256 MiB limit if that is lower, then give
# back 734 blocks of 64 KiB: the room the second call fits in only without
# the first call's stack.
keep = []
try:
    while len(keep) < 5120:
        keep.append(bytearray(1 << 16))
except MemoryError:
    pass
del keep[-734:]
gc.collect()
out = []


def run(fn):
    try:
        out.append(fn())
    except MemoryError:
        out.append('MemoryError')


run(lambda: len(dict(**base)))
run(lambda: f(**base, **other))
keep = None
gc.collect()
print(zeros, out[0])
