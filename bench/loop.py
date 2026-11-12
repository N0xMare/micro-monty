# loop: a module-level counting loop, summing range(3_000_000).
total = 0
for i in range(3_000_000):
    total += i
print(total)
