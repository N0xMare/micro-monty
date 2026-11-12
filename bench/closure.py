# closure: 1M calls of a closure that reads a captured variable.
def make_adder(n):
    def add(x):
        return x + n

    return add


add = make_adder(3)
total = 0
for i in range(1_000_000):
    total = add(total)
print(total)
