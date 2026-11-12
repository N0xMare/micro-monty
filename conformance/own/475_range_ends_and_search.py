# Ranges at the i64 ends, longer than i64::MAX, reversed and sliced, and
# range.count/range.index.
MIN, MAX = -2**63, 2**63 - 1
R = range(MIN, MAX)


def show(f):
    try:
        r = f()
    except Exception as e:
        r = (type(e).__name__, str(e))
    print(r)


# reversed(): the iterator keeps a count, so it ends at either i64 end
show(lambda: next(reversed(R)))
show(lambda: next(reversed(range(-1, MAX))))
show(lambda: next(reversed(range(MIN, MAX, 2)), 'empty'))
show(lambda: list(reversed(range(MIN, MAX, MAX))))
show(lambda: list(reversed(range(MAX, MIN, -MAX))))
show(lambda: list(reversed(range(MIN, MIN + 3))))
show(lambda: list(reversed(range(MAX - 3, MAX))))
show(lambda: list(reversed(range(5, -6, MIN))))
show(lambda: list(reversed(range(-5, 6, MAX))))
show(lambda: list(reversed(range(10, 0, -3))))
show(lambda: list(reversed(range(0))))
show(lambda: type(reversed(range(3))).__name__)
show(lambda: sum(reversed(range(MIN, MIN + 3))))

# forward iteration past the last item, in a loop and by next()
show(lambda: [i for i in range(MAX - 1, MAX, 10)])
show(lambda: [i for i in range(MIN + 1, MIN, -10)])
show(lambda: list(range(MAX - 1, MAX, 10)))
it = iter(range(MAX - 2, MAX))
show(lambda: (next(it), next(it), next(it, 'done'), next(it, 'done')))
n = 0
for i in range(MAX - 1, MAX, 10):
    n += 1
    if n > 3:
        break
print(n)

# slices of a range longer than i64::MAX
show(lambda: R[5:])
show(lambda: R[::2])
show(lambda: R[-3:])
show(lambda: R[2**63:])
show(lambda: R[2**64:])
show(lambda: len(R[1:]))
show(lambda: R[1:][0])
show(lambda: range(10)[::3])
show(lambda: range(10)[8:2:-2])
show(lambda: range(10)[::0])

# count and index
show(lambda: range(5).count(1))
show(lambda: range(5).count(5))
show(lambda: range(5).index(3))
show(lambda: range(5).index(7))
show(lambda: range(0, 10, 3).index(9))
show(lambda: range(10, 0, -3).index(4))
show(lambda: range(10, 0, -3).index(5))
show(lambda: range(5).index(True))
show(lambda: range(5).count(False))
show(lambda: R.index(MAX - 1))
show(lambda: R.count(MAX - 1))
show(lambda: R.count(MIN))
show(lambda: R.count(MAX))
show(lambda: range(5).index(2**100))
show(lambda: range(5).count(-2**100))
show(lambda: range(1, 2**40, 7).index(7 * 10**9 + 1))
show(lambda: range(5).index('a'))
show(lambda: range(5).count('a'))
show(lambda: range(5).index(2.0))
show(lambda: range(5).count(2.0))
show(lambda: range(5).index(1, 2))
show(lambda: range(5).count())
show(lambda: range(5).index(x=3))
show(lambda: range.index(3, 3))
show(lambda: 'count' in dir(range) and 'index' in dir(range))


class Three:
    def __eq__(self, o):
        return o == 3

    __hash__ = None


class Int(int):
    pass


show(lambda: range(5).index(Three()))
show(lambda: range(10).count(Three()))
show(lambda: range(5).index(Int(4)))
show(lambda: range(5).count(Int(9)))
show(lambda: range(20000).count(1.5))

# a step of i64::MIN: the length is computed in i128
W = range(MAX, MIN, MIN)
show(lambda: (len(W), list(W), list(reversed(W)), W.index(-1), W[1]))
