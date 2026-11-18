# Identity-hashed keys made in sequence (the GC design review's bulk.py):
# a lookup and a table rebuild get the same free probes, so fuel does not
# follow where a snapshot's collection moved the objects.
# identity keys, then whole-table builds: copies and constructors
class C:
    pass
N = 3000
keep = []
for i in range(N):
    junk = [i, i + 1, i + 2]
    keep.append(C())
n = 0
for r in range(3):
    s = set(keep)
    t = s.copy()
    u = s | t
    d = dict.fromkeys(keep)
    e = dict(d)
    f = {k: 1 for k in keep}
    n += len(s) + len(t) + len(u) + len(d) + len(e) + len(f)
print(n)
