# A stepped set operation's lookup along a chain longer than one step's
# walk (262,136 int keys on the probe path of 'zz') stops part way and
# goes on in the next step (AE1): parks and restores land inside it.
mask = (1 << 19) - 1


def path(q, n):
    q &= (1 << 64) - 1
    i = (q ^ (q >> 32)) & mask
    p, seen, out = q, set(), []
    while len(out) < n:
        if i not in seen:
            seen.add(i)
            out.append(i)
        p >>= 5
        i = (5 * i + 1 + p) & mask
    return out


dd = dict.fromkeys(path(hash('zz'), 262_136))
zs = ['zz', 'zz']
print(dd.keys() & zs, dd.keys().isdisjoint(zs), dd.keys().isdisjoint(iter(zs)))
print({'zz'} <= dd.keys(), len(dd.keys() - zs))
dd['zz'] = 1
print(dd.keys() & zs, dd.keys().isdisjoint(iter(zs)), {'zz'} <= dd.keys(), len(dd.keys() - zs))
