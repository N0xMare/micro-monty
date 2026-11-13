# Over 32 MiB live a collection sweeps lazily. A snapshot's collection
# sweeps at once, segment by segment as a restore sweeps, so the session
# that goes on after a snapshot lists its free blocks as the restored one
# does and both hand out the same blocks after it: the durability gate's
# restore-against-snapshot comparison checks this case's final state.
# Before, the lazy sweep was finished in reverse segment order, so `id()`
# values and set orders after a snapshot differed between the two.
class C:
    pass


keep = [C() for _ in range(600_000)]
del keep[::2]
junk = 0
for i in range(200_000):
    junk += i
fresh = [C() for _ in range(8)]
s = set(fresh)
print(len(keep), junk, len(s), sorted(fresh.index(x) for x in s))
