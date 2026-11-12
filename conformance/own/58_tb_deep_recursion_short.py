def recurse(n):
    if n == 0:
        return 1 / 0
    return recurse(n - 1)
recurse(10)
