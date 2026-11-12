from itertools import groupby
r = [(k, len(list(g))) for k, g in groupby('aabbbc')]
print(r)
