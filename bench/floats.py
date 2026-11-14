# floats: **, negation and comparisons on floats. The values are dyadic
# (n / 8, n / 32) and stay exact, so every engine prints the same integers.
above = 0
negatives = 0
acc = 0.0
powers = 0.0
for i in range(1, 200_001):
    x = (i % 1000) / 8
    y = -x
    sq = x ** 2
    cu = (x / 4) ** 3
    if sq > cu:
        above += 1
    if y <= -50.0:
        negatives += 1
    acc += sq - cu
    powers += 2.0 ** (i % 20) - -y
print(above, negatives, int(acc * 32768), int(powers))
