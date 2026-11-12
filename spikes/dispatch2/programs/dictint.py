# dictint: insert 200k int keys, then look each one up; module level.
d = {}
for i in range(200_000):
    d[i] = i
total = 0
for i in range(200_000):
    total += d[i]
n = len(d)
print(n, total)
