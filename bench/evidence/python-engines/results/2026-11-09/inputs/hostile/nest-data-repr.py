x = []
for _ in range(1000000):
    x = [x]
print(len(repr(x)))
