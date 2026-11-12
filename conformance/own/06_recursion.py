def fact(n):
    return 1 if n <= 1 else n * fact(n - 1)

print(fact(20))

def ack(m, n):
    if m == 0:
        return n + 1
    if n == 0:
        return ack(m - 1, 1)
    return ack(m - 1, ack(m, n - 1))

print(ack(2, 3))

def even(n):
    return True if n == 0 else odd(n - 1)

def odd(n):
    return False if n == 0 else even(n - 1)

print(even(100), odd(7))

def forever(n):
    return forever(n + 1)

try:
    forever(0)
except RecursionError as e:
    print("RecursionError:", e)
print("still alive", fact(5))
