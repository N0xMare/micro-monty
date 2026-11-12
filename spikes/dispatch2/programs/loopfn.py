# loopfn: callfn's loop with the call replaced by its body.
def main():
    total = 0
    for i in range(1_000_000):
        total = total + 1
    return total


print(main())
