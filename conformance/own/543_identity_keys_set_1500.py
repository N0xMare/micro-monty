# Identity-hashed keys made in sequence (the GC design review's sh_inst_set_1500.py):
# a lookup and a table rebuild get the same free probes, so fuel does not
# follow where a snapshot's collection moved the objects.
class C:
    def m(self): pass
keepc = C()
N = 1500
t = set()
keep = []
for i in range(N):
    junk = [i, i + 1, i + 2]
    o = C()
    keep.append(o)
    t.add(o)
hits = 0
for o in keep:
    if o in t:
        hits += 1
print(len(t) > 0, hits > 0)
