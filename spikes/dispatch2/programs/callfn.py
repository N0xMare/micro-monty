# callfn: 1M calls of a plain global function from inside a function.
def inc(x):
    return x + 1


def main():
    total = 0
    for i in range(1_000_000):
        total = inc(total)
    return total


print(main())
