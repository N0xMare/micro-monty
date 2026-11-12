# A slice of a short list assigned a value of 65,536 items or more: its
# items were gathered a chunk at a time and handed to the prelude, which
# stored them as a list and came back to the same native, until
# RecursionError. A long list or tuple value now goes the long way itself.
N = 65536


class L(list):
    pass


class T(tuple):
    pass


def kinds():
    return [
        list(range(N)),
        tuple(range(N)),
        range(N),
        (i for i in range(N)),
        iter(list(range(N))),
        set(range(N)),
        {i: i for i in range(N)}.values(),
        zip(range(N)),
        map(int, range(N)),
        'x' * N,
        L(range(N)),
        T(range(N)),
    ]


for v in kinds():
    x = [1, 2]
    x[:] = v
    print(len(x), x[0], x[-1])

for v in kinds():
    x = [1, 2, 3]
    x[1:2] = v
    print(len(x), x[0], x[-1])

for v in kinds():
    x = list(range(5))
    try:
        x[::2] = v
    except ValueError as e:
        print(e)

x = L([1, 2])
x[:] = range(N)
print(type(x).__name__, len(x))
x = [1, 2]
x[:1] = x * (N // 2)
print(len(x))
x = list(range(5))
try:
    x[4:0:-1] = tuple(range(N))
except ValueError as e:
    print(e)
