# `copy.copy` and `copy.deepcopy` of exceptions (user and builtin) and of
# set subclasses reduce as CPython's do: the class called
# with the arguments or the items, then the instance's attributes; and
# `dataclasses.asdict` copies them the same way.
import copy
import dataclasses


class E2(Exception):
    pass


class SS(set):
    pass


class MyE(Exception):
    def __init__(self, a, b):
        super().__init__(a)
        self.b = b


class KE(KeyError):
    pass


def show(f):
    try:
        r = f()
        print(type(r).__name__, repr(r), getattr(r, 'args', None), getattr(r, '__dict__', None))
    except Exception as e:
        print('raised', type(e).__name__, e)


for x in (E2('x', 1), E2(), KeyError('k'), StopIteration(3), ValueError('a', 1), KE('q'),
          ZeroDivisionError('z')):
    show(lambda: copy.copy(x))
    show(lambda: copy.deepcopy(x))
try:
    raise E2('boom')
except E2 as ex:
    print(str(copy.deepcopy(ex)), str(copy.copy(ex)), copy.copy(ex).__traceback__)
s = SS([1, 2])
s.tag = 't'
show(lambda: copy.deepcopy(s))
show(lambda: copy.copy(s))
show(lambda: copy.deepcopy(SS([frozenset([3]), (1, 2)])))
show(lambda: copy.deepcopy(MyE(1, 2)))
e = E2('z')
e.note = [1]
c = copy.deepcopy(e)
print(c.args, c.note, c.note is e.note, copy.copy(e).note is e.note)
inner = [1]
t = SS([(1, 2)])
d = copy.deepcopy({'s': t, 'again': t, 'l': inner})
print(d['s'] is d['again'], d['s'] == t, d['s'] is t)


class G(set):
    def __getstate__(self):
        return {'g': len(self)}


print(copy.copy(G([1, 2, 3])).__dict__, sorted(copy.deepcopy(G([9]))))


@dataclasses.dataclass
class DC:
    a: object
    b: object


print(dataclasses.asdict(DC(E2('q', 2), SS([5]))))
print(dataclasses.asdict(DC(KeyError('k'), [SS([1]), SS([2])])))
