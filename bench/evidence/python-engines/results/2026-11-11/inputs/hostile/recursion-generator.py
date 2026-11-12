def g():
    yield from g()
list(g())
