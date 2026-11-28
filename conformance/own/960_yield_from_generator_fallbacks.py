# The delegate can already be running, or be exhausted by another driver
# while yield from is suspended. These are SEND cases, not direct .send().
def reentrant():
    try:
        yield from running
    except ValueError as exc:
        yield str(exc)


running = reentrant()
assert next(running) == 'generator already executing'
assert next(running, 'done') == 'done'


def leaf():
    yield 'first'
    return 'taken by the other driver'


def delegate(it):
    return (yield from it)


child = leaf()
outer = delegate(child)
assert next(outer) == 'first'
assert next(child, 'done') == 'done'
try:
    outer.send('after exhaustion')
except StopIteration as exc:
    assert exc.value is None
else:
    raise AssertionError('exhausted delegation returned a value')
