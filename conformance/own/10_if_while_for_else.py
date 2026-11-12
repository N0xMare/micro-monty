for n in range(2, 20):
    for d in range(2, n):
        if n % d == 0:
            break
    else:
        print(n, end=" ")
print()

i = 0
while i < 10:
    i += 1
    if i % 2:
        continue
    if i == 8:
        break
    print("even", i)
else:
    print("never")

while False:
    pass
else:
    print("while-else ran")

for x in []:
    pass
else:
    print("for-else on empty")

x = 7
if x < 5:
    print("small")
elif x < 10:
    print("medium")
else:
    print("large")
print("yes" if x else "no", "a" if not x else "b")
