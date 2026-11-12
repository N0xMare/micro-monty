# Generators and objects on paths no gate reached: yield from an iterator
# with a send method (and one without), throw with a type, raise from
# non-exceptions, regex conditionals and non-ASCII literals, mapping
# patterns on negative, float, bytes and complex keys, the reprs of
# methods, cells and coroutines, and wait_for on a future never set.

print('-- send_yield_from')
class It:
    def __init__(self):
        self.n = 0
    def __iter__(self):
        return self
    def __next__(self):
        self.n += 1
        if self.n > 3:
            raise StopIteration(self.n)
        return self.n
    def send(self, v):
        print('send', v)
        return self.__next__()
def g():
    r = yield from It()
    print('ret', r)
x = g()
print(next(x), x.send('a'), x.send('b'))
try:
    x.send('c')
except StopIteration:
    print('stop')
class NoSend:
    def __iter__(self): return self
    def __next__(self): return 1
def h():
    yield from NoSend()
y = h()
next(y)
try:
    y.send(5)
except (TypeError, AttributeError) as e:
    print(type(e).__name__, e)

print('-- throw_forms')
def g():
    try:
        yield 1
    except ValueError as e:
        print('caught', repr(e))
        yield 2
x = g(); next(x)
print(x.throw(ValueError('v')))
x = g(); next(x)
print(x.throw(ValueError))

print('-- raise_from_bad')
for c in (5, 'x', None, ValueError, KeyError('k')):
    try:
        raise RuntimeError('r') from c
    except TypeError as e:
        print('TypeError', e)
    except RuntimeError as e:
        print(repr(e.__cause__), e.__suppress_context__)

print('-- re_cond')
import re
print(re.match(r'(a)?(?(1)b|c)', 'ab'), re.match(r'(a)?(?(1)b|c)', 'c'), re.match(r'(?:(a)|x)(?(1)b|y)', 'xy'))
print(re.findall(r'(<)?\w+(?(1)>)', '<a> b <c'))
print(re.search('é', 'café'), re.search('€x', 'a€x'), re.findall('ü', 'üü'))

print('-- match_keys')
for v in ({-1: 'a'}, {1.5: 'b'}, {b'k': 'c'}, {-2.5: 'd'}, {1j: 'e'}, {-1-1j: 'f'}, {2+3j: 'g'}):
    match v:
        case {-1: x}: print('neg', x)
        case {1.5: x}: print('float', x)
        case {b'k': x}: print('bytes', x)
        case {-2.5: x}: print('negf', x)
        case {1j: x}: print('cplx', x)
        case {-1-1j: x}: print('ncplx', x)
        case {2+3j: x}: print('sum', x)

print('-- gen_repr')
def g():
    yield 1
async def ag():
    yield 1
r = repr(g())
print(r.startswith('<generator object g at 0x'), repr(ag()).startswith('<async_generator object ag at 0x'))
def outer():
    def inner():
        yield
    return inner()
print(repr(outer()).split(' at ')[0])

print('-- reprs')
import re
def show(r):
    print(r.split(' at 0x')[0] + ('>' if ' at 0x' in r else ''))
show(repr([].append))
show(repr('a'.upper))
show(repr(len))
async def co():
    pass
c = co()
show(repr(c))
c.close()
show(repr(staticmethod(len)))
show(repr(classmethod(len)))
def outer():
    x = 1
    def inner():
        return x
    return inner

class A:
    def m(self):
        pass
show(repr(A().m))

import sys
show(repr(sys))
show(repr(iter([])))
show(repr(iter({})))
show(repr({}.keys()))

show(repr(zip()))
show(repr(enumerate([])))
show(repr(reversed([])))

print('-- aio_deadlock')
import asyncio
async def main():
    f = asyncio.get_running_loop().create_future()
    try:
        await asyncio.wait_for(f, 0.01)
    except TimeoutError:
        print('timeout')
asyncio.run(main())
