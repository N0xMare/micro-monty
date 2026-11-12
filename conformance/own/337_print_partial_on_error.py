# print writes each argument and separator as it goes: when str() of a later
# argument raises, the earlier ones are already out (CPython's order).
try:
    print("a", 10 ** 5000)
except ValueError:
    print("EXC")
try:
    print("x", 1, [2], 10 ** 5000, "never", sep="-", end="!")
except ValueError:
    print("|EXC")
try:
    print(10 ** 5000, "b")
except ValueError:
    print("EXC")
try:
    print("y", [10 ** 5000])
except ValueError:
    print("EXC")
print("done")
