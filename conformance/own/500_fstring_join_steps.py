# An f-string whose parts pass a step's work (8 MiB) joins them a step at
# a time, as `''.join` does (docs/spec.md §5): str, non-ASCII and bytes
# fields alike. Sized just over the threshold so a park lands inside the
# join; results must equal one native run's.


def h(t):
    m = len(t) // 2
    return len(t), t[:8], t[m - 4:m + 4], t[-8:]


s = 'ab' * 2_100_000
u = 'xé' * 1_400_000
B = b'q' * 4_200_000
print(h(f'<{s}|{s}>'), f'{s}{s}' == s + s)
print(h(f'{u}{s}'), h(f'{s}-{u}-{1}'))
print(h(f'<{B}>{B!r:.20}'), h(f'{B}{B!r}'))
n = 0
for k in range(3):
    t = f'{k}:{s}{s}'
    n += len(t) + ord(t[0])
print(n, f'{s}{s}' is s, f'{"x"}{s[:3]}')
