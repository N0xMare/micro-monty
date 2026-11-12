# sort: list comprehensions (filtered, nested, over tuples) and sorting 50k
# records with key functions: a lambda, a tuple key, reverse, and a stable
# re-sort on a key with many ties.
records = [(i * 7919 % 50000, 'n' + str(i * 31 % 1000), i % 10) for i in range(50000)]
by_value = sorted(records, key=lambda r: r[0])
by_name = sorted(records, key=lambda r: (r[1], -r[0]))
by_bucket = sorted(by_value, key=lambda r: r[2], reverse=True)
names = [r[1] for r in by_name if r[2] < 5]
words = sorted(dict.fromkeys(names), key=len)
grid = [[x * y % 10 for x in range(100)] for y in range(100)]
flat = sorted([v for row in grid for v in row if v])

check = 0
for i, r in enumerate(by_value):
    check = (check * 31 + r[0] + i) % 1000000007
for i, r in enumerate(by_name[:1000]):
    check = (check * 31 + r[0] * (i + 1)) % 1000000007
for i, r in enumerate(by_bucket):
    check = (check * 31 + r[0] + r[2]) % 1000000007
print(check, len(names), len(words), words[0], words[-1], len(flat), sum(flat[::97]))
