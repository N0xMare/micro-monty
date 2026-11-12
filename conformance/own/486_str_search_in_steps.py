# `find`, `rfind`, `index`, `rindex` and `in` over a long str window
# search a step at a time (`STR_SCAN` kinds `FIND` and `RFIND`): a
# match across two steps' windows, at either end, with a long pattern or
# a short one whose first or last byte is everywhere, in ASCII and
# non-ASCII text, with `start` and `end`.

N = 700_000
s = 'a' * N
print(s.find('ab'), s.rfind('ba'), 'ab' in s, 'ba' not in s)
print(s.find('a' * 9 + 'b'), s.rfind('b' + 'a' * 999), ('a' * 999 + 'b') in s)
t = s + 'b'
print(t.find('ab'), t.rfind('ab'), t.index('a' * 1000 + 'b'), 'ab' in t)
u = 'b' + s
print(u.find('ba'), u.rfind('ba'), u.rindex('b' + 'a' * 1000), 'ba' in u)
print(t.find('ab', 10, N), t.find('ab', 10, N + 1), t.rfind('a', 5, 9))
print(s.find('aa', N - 1), s.rfind('aa', 0, 1), s.find('', N), s.rfind(''))

e = 'é' * N + '€' + 'é' * N
print(e.find('é€'), e.rfind('€é'), e.index('é' * 10 + '€'), '€é' in e)
print(e.find('€', N + 1), e.rfind('€', 0, N), e.find('€', -N - 5, -N + 5))

w = (''.join(chr(97 + (i * 7 + i // 13) % 26) for i in range(4999)) * 141)[:N]
for p in ['xyz', w[N // 2:N // 2 + 50], w[-30:], w[:30], 'q' * 9, w[1:400]]:
    print(w.find(p), w.rfind(p), p in w, w.count(p))

for f in (s.index, s.rindex):
    try:
        f('b')
    except ValueError as x:
        print('ValueError', x)
try:
    print(1 in s)
except TypeError as x:
    print('TypeError', x)


class S(str):
    pass


print(S('ab') in t, t.find(S('ab')))
