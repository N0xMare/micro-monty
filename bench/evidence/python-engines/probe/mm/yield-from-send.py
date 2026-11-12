def g():
    x = yield 1
    yield x * 2
it = g()
next(it)
r = it.send(5)
print(r)
