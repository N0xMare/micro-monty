# 3,000 named tuples grow the code table, its caches, the signature index
# and the shapes by doubling, then memory is filled: a session restored
# after them must have the memory the session that went on has
# (restore-vs-snapshot; vector capacities are charged, step 4).
from collections import namedtuple

kinds = [namedtuple('N', [f'f{i}', 'shared']) for i in range(3000)]
junk = 0
for i in range(20000):
    junk += i


# Fill up to 320 MiB, or the 256 MiB memory limit if that is lower. The
# counts depend on the memory the session has left, so they stay in
# globals for the final-state digest and are not printed. A park this
# near the limit is refused on the target that charges more (`cross`,
# step 4).
CAP = 320 << 20
counts = [0, 0, 0]
keep = []
total = 0
for k, size in enumerate((1 << 20, 1 << 16, 1 << 10)):
    try:
        while total < CAP:
            keep.append(bytearray(size))
            counts[k] += 1
            total += size
    except MemoryError:
        pass
keep = None
print(len(kinds), junk, counts[0] > 0)
