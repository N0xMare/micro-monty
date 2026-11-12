# Builtin paths no gate reached: OSError's errno fields, short bytes
# searches, int subclasses' truth, str.format index errors, ordering
# containers of unorderable items, ** of a non-mapping, del/setattr on
# exceptions and functions, dict() of bad pairs, dir() and attribute
# suggestions per builtin type, keyword-only natives, %a, format grouping.

print('-- oserror')
e = OSError(2, 'No such file or directory', 'a.txt')
print(e.errno, e.strerror, e.filename, str(e))
e = OSError(2, 'No such file', 'a', None, 'b')
print(e.filename2, str(e))
print(str(OSError(1, 'x')), OSError(1).errno, OSError(1, 2, 3, 4, 5, 6).errno)
print(str(FileNotFoundError(2, 'missing')), FileNotFoundError(2, 'm', 'f').filename)

print('-- bytes_find_short')
b = b'abcabcabc'
print(b.find(b'bc'), b.rfind(b'bc'), b.find(b'ca', 2), b.rfind(b'ab', 0, 5), b.index(b'cab'), b.rindex(b'abc'), b.count(b'bc'))
print(bytearray(b'xyzxyz').find(b'yz'), bytearray(b'xyzxyz').rfind(b'yz'))

print('-- bool_int_sub')
class I(int): pass
print(bool(I(0)), bool(I(5)), bool(I(2**70)), not I(0))

print('-- format_index')
for s, a, k in (('{} {}', (1,), {}), ('{1}', (1,), {}), ('{x}', (), {}), ('{0[1]}', ([5],), {})):
    try:
        print(s.format(*a, **k))
    except (IndexError, KeyError) as e:
        print(type(e).__name__, e)

print('-- cmp_containers')
for f in (lambda: [{}] < [{}], lambda: [{}] <= [{}], lambda: [{}, 1] < [{}, 2], lambda: [{1: 2}] < [{1: 3}], lambda: ({}, 1) < ({}, 2), lambda: [set()] < [frozenset()]):
    try:
        print(f())
    except TypeError as e:
        print(e)

print('-- dict_update_op')
def f(**k): return k
for g in (lambda: f(**[1]), lambda: {**[1]}, lambda: {**{1: 2}}, lambda: f(**{1: 2}), lambda: f(a=1, **{"a": 2}), lambda: f(**{"a": 1}, **{"a": 2})):
    try:
        print(g())
    except TypeError as e:
        print(e)

print('-- delitem_delattr')
class I:
    def __index__(self): return 0
l = [1, 2, 3]
del l[I()]
print(l)
for f in (lambda: l.__delitem__('a'), lambda: (1,).__delitem__(0)):
    try: f()
    except (TypeError, AttributeError) as e: print(type(e).__name__, e)
t = (1, 2)
try:
    del t[0]
except TypeError as e:
    print(e)
e = ValueError()
e.foo = 1
del e.foo
try:
    del e.bar
except AttributeError as x:
    print(x)
e.__cause__ = KeyError('k')
e.__context__ = TypeError('t')
print(repr(e.__cause__), repr(e.__context__), e.__suppress_context__)
def f(a, *, b=2): return a, b
f.__kwdefaults__ = {'b': 5}
print(f(1))
try:
    f.__kwdefaults__ = 3
except TypeError as x:
    print(x)
f.__kwdefaults__ = None
try:
    f(1)
except TypeError as x:
    print(x)
f.__dict__ = {'z': 1}
print(f.z)

print('-- dict_fill_misc')
for f in (lambda: dict([(1,)]), lambda: dict(['ab', 'c']), lambda: ord(5), lambda: ord(b'ab'), lambda: b'a'.center(5, b'xy'), lambda: b'a'.center(5, 'x'), lambda: b'a'.ljust(3, bytearray(b'-')), lambda: [1] * 1.5, lambda: 'a' * 'b', lambda: (1,) * None):
    try:
        print(f())
    except Exception as e:
        print(type(e).__name__, e)
class I:
    def __index__(self):
        return 2
print([1] * I(), I() * (0,), 'ab' * I(), b'x' * I())

print('-- attr_suggest')
for v, n in (({}, 'iterms'), ((), 'cont'), (b'', 'decod'), (set(), 'ad'), (frozenset(), 'unon'), (1.5, 'is_integr'), (1j, 'conjugat'), (ValueError(), 'add_not'), (range(1), 'strat')):
    try:
        getattr(v, n)
    except AttributeError as e:
        print(e)

print('-- dir_methods')
for v in ({}, (), b'', set(), frozenset(), 1j, ValueError(), range(1), [], 1, True):
    print(type(v).__name__, [n for n in dir(v) if not n.startswith('_')])

print('-- native_kw_missing')
for f in (lambda: 'a b'.split(maxsplit=1), lambda: 'aXb'.replace('X', 'Y', count=1), lambda: (5).to_bytes(byteorder='big'), lambda: 'abc'.startswith(), lambda: divmod(1), lambda: sorted(key=len), lambda: round(ndigits=1), lambda: pow(exp=2, base=3), lambda: (1).to_bytes(length=2)):
    try:
        print(f())
    except TypeError as e:
        print(e)

print('-- percent_a')
class R:
    def __repr__(self):
        return 'Ré'
print('%a %a' % (R(), 'é'), '%r' % R(), ascii(R()), ascii([R()]), '%a' % ([R(), 'ü'],))
print(b'%a' % 'é', b'%a' % R())

print('-- inst_attr_names')
class A:
    pass
a = A()
for i in range(40):
    setattr(a, f'x{i}', i)
del a.x3
print(len(vars(a)), sorted(dir(a))[:3], [n for n in dir(a) if n.startswith('x')][:5])
try:
    a.x3
except AttributeError as e:
    print(e)
try:
    a.x_4
except AttributeError as e:
    print(e)

print('-- strfmt_misc')
print(format(1234567.891, ',.2f'), format(1234567.891, '_.3f'), format(-1234.5, '+,.1f'), format(1e20, ',f'), format(12345.6789, ',.10f'))
print(format(1234, ',d'), format(1234.5, '015,.1f'), format(-12, '08d'), format(12, '=+8'), format(1.5, '^+10.2f'))

print('-- unicode_float')
print(float('١٢٣'), float(' \u0663.\u0661 '), int('٣'), float('１.５'), float('\u2003 7 \u2003'))

print('-- exc_ctor_misc')
for e in (KeyError(), KeyError('a'), KeyError('a', 'b'), OSError(5, 'x', None), UnicodeError('u'), StopIteration(1), SystemExit(3)):
    print(repr(e), str(e), e.args)
