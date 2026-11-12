# dict: insert 200k string keys, then look each one up.
keys = ['key' + str(i) for i in range(200000)]
d = {}
i = 0
for k in keys:
    d[k] = i
    i += 1
total = 0
for k in keys:
    total += d[k]
print(len(d), total)
