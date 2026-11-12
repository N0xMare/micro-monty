fs = 'f'
for _ in range(24):
    fs = frozenset([fs, (fs, 1)])
{0: 1}[fs]
