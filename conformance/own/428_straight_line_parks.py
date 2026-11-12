# Straight-line code whose instructions do long native work (each charges
# more than 64 fuel units) checks fuel, deadlines and requests after each
# such instruction, not only at jumps and calls: a park lands between two
# statements and the run goes on with the same values.
s = 'ab' * 50_000
t = 'ab' * 49_999 + 'ac'
l = list(range(20_000))
r = []
r.append(s == t)
r.append(s < t)
r.append('ac' in s)
r.append(len(s + 'x'))
r.append(len(s[1:]))
r.append(len([*l]))
r.append(l == l[:])
r.append(len(f'{s}{t}'))
r.append(s.count('b'))
u = s * 3
r.append(len(u))
v = u.replace('a', 'xy')
r.append(len(v))
w = tuple(l) + tuple(l)
r.append(len(w))
d = dict.fromkeys(l)
r.append(len(d))
e = set(l) ^ set(range(10_000, 30_000))
r.append(len(e))
print(r)
