def f(n):
    return f(n + 1)
try:
    f(0)
except RecursionError:
    print("caught")
