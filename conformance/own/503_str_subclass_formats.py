# A str subclass as `%`'s bare right operand is formatted by its own
# `__str__` / `__repr__`, as CPython's `PyUnicode_Format` gets the object;
# `%b` takes a bytes subclass's value before `__bytes__`. A not-found
# `partition` gives back the receiver itself, and a format spec that leaves
# the text as it is gives the object itself, a subclass instance too
# (CPython's writer); `replace` and `split` give a new str. Results, types
# and identities must equal CPython's.


class S(str):
    def __str__(self):
        return 'STR'

    def __repr__(self):
        return 'REPR'


class T(str):
    pass


s = S('x')
print(repr('%s' % s), repr('%5s|' % s), repr('%r' % s), repr('%a' % s), repr('%s' % (s,)))
print(repr('%-6r|%s' % (s, s)), repr(T('%s!') % s), repr(T('%s!') % T('y')))
y = T('v=%s')
y %= s
print(repr(y), type(y).__name__)


class E(str):
    def __str__(self):
        return 'E.str'


e = E('e')
print(repr('%s' % e), repr('%r' % e), repr('%5s' % e), repr('%s' % (e,)), repr('%r' % (e,)), repr('%5s' % (e,)),
      repr('%s|%r' % (e, e)), repr('%(k)s' % {'k': e}))

big = 'p' * 70_000 + '%s|%r'
print(len(big % (s, s)), (big % (s, s))[-9:])


class D(dict):
    def __getitem__(self, k):
        return 'D[' + k + ']'


class TS(tuple):
    pass


class G(str):
    def __getitem__(self, k):
        return 'G'


def attempt(f):
    try:
        return repr(f())
    except TypeError as e:
        return 'TypeError: ' + str(e)


for f in (lambda: '%(a)s' % D(a=1), lambda: '%s-%s' % TS((1, 2)), lambda: '%s' % TS(()),
          lambda: 'x' % TS(()), lambda: '%s' % G('g'), lambda: '%(x)s' % G('g'),
          lambda: 'x' % G('g'), lambda: 'q' * 600_000 % TS((1,))):
    print(attempt(f))


class B(bytes):
    def __bytes__(self):
        return b'BYT'


class P:
    def __bytes__(self):
        return b'PB'


print(b'%b' % (B(b'q'),), b'%s' % (B(b'q'),), b'%b' % B(b'q'), b'%b|%b' % (P(), B(b'r')))
print(b'%r' % B(b'q'), attempt(lambda: b'%b' % (T('z'),)))

for n in (10, 600_000):
    t = T('a' * n)
    e = 'a' * n
    for name, r, o in [
        ('partition', t.partition('zz')[0], t), ('rpartition', t.rpartition('zz')[2], t),
        ('partition found', t.partition('a')[2], t), ('str.partition', str.partition(t, 'zz')[0], t),
        ('e partition', e.partition('zz')[0], e), ('replace', t.replace('zz', 'y'), t),
        ('split', t.split('zz')[0], t), ('rsplit', t.rsplit('zz')[0], t),
        ('format s', format(t, 's'), t), ('format empty', format(t, ''), t),
        ('format <', format(t, '<'), t), ('format width', format(t, str(n)), t),
        ('format wider', format(t, str(n + 1)), t), ('format prec', format(t, '.%d' % n), t),
        ('format prec cut', format(t, '.%d' % (n - 1)), t), ('format fill', format(t, '*^1'), t),
        ('__format__', t.__format__('s'), t), ('__format__ empty', t.__format__(''), t),
        ('e format <', format(e, '<'), e), ('e format width', format(e, '5'), e),
        ('fstring', f'{t}', t), ('fstring s', f'{t:s}', t), ('fstring nested', f'{t:{"s"}}', t),
        ('str.format', '{:s}'.format(t), t), ('str.format 0', '{0:<}'.format(t), t),
        ('str.format map', '{a:s}'.format_map({'a': t}), t), ('str.format empty', '{}'.format(t), t),
        ('str.format conv', '{!s:s}'.format(t), t), ('str.format after', '{}{:s}'.format('', t), t),
        ('str.format before', '{:s}{}'.format(t, ''), t), ('str.format text', 'x{:s}'.format(t), t),
        ('e str.format', '{}'.format(e), e), ('e str.format conv', '{!s}'.format(e), e),
    ]:
        print(n, name, type(r).__name__, r is o, len(r))

print(type(format(T(''), 's')).__name__, format(S('xy'), ''), format(S('xy'), 's'), type(format(S('xy'), 's')).__name__)
print(repr('{:s}'.format(S('xy'))), type('{:2}'.format(S('xy'))).__name__, f'{S("xy")}', f'{S("xy"):3}|')

# An exact str's no-op replace, and a split or splitlines with the whole
# text as its one piece, are the receiver itself.
e = 'abcab' * 5
x = 'ab'
print(e.replace('zz', 'y') is e, e.replace('ab', 'y', 0) is e, e.replace(x, x) is e, ''.replace('', 'x'), e.replace('', '', 0) is e)
print(e.split('zz')[0] is e, e.rsplit('zz')[0] is e, e.split()[0] is e, e.rsplit()[0] is e, (' ' + e).split()[0] is e,
      e.splitlines()[0] is e, e.split('zz', 0)[0] is e, e.split(None, 0)[0] is e, e.rsplit(None, 0)[0] is e, (e + ' ').split(None, 0)[0] is e)
print(e.replace('ab', 'y') is e, e.replace('ab', 'y'), e.split('ab'), ''.split(), ''.split('x'), ''.splitlines(), 'a\nb'.splitlines())


# `__format__` returning a str subclass (review M1): an empty field is a
# plain '', a non-empty lone one is the object itself, and a nested spec
# reaches the formatter as an exact str; `format()`, `__format__` and an
# f-string's nested field read a str subclass spec as its text.
class F:
    def __init__(self, r):
        self.r = r

    def __format__(self, spec):
        return self.r


class G:
    def __format__(self, spec):
        return 'G[%s:%s]' % (type(spec).__name__, spec)


emp, x = T(''), T('x')


def show(f):
    try:
        r = f()
        return '%r:%s:%s%s' % (r, type(r).__name__, r is emp, r is x)
    except Exception as e:
        return '%s: %s' % (type(e).__name__, e)


for f in (lambda: '{}'.format(F(emp)), lambda: '{0}'.format(F(emp)), lambda: '{:>6}'.format(F(emp)),
          lambda: '{a}'.format(a=F(emp)), lambda: '{a}'.format_map({'a': F(emp)}), lambda: '{}{}'.format(F(emp), F(emp)),
          lambda: '{}'.format(F(x)), lambda: '{}{}'.format(F(emp), F(x)), lambda: '{}{}'.format(F(x), F(emp)),
          lambda: 'a{}'.format(F(emp)), lambda: '{}a'.format(F(emp)), lambda: '{{{}'.format(F(emp)),
          lambda: '{:{}}'.format(F(emp), ''), lambda: '{:{}}'.format(F(x), F(emp)), lambda: '{:{}}'.format(5, F(T(''))),
          lambda: '{:{}}'.format(5, F(T('>3'))), lambda: '{:{w}}'.format(5, w=F(T('>3'))), lambda: '{:{}}'.format(5, T('>3')),
          lambda: '{:{!s}}'.format(5, T('>3')), lambda: '{:>{}}'.format(5, F(T('3'))),
          lambda: '{a:{b}}'.format_map({'a': 5, 'b': F(T('>3'))}), lambda: '{:{}}'.format(1.5, F(T('.3f'))),
          lambda: '{:{}}'.format('ab', F(T('>5'))), lambda: '{:{}}'.format(F('ok'), F(T('zz'))),
          lambda: '{:{}}'.format(G(), F(T('>4'))), lambda: '{:{}x}'.format(G(), F(T('>4'))),
          lambda: f'{F(emp)}', lambda: f'{F(x)}', lambda: f'{F(emp)}{F(emp)}', lambda: f'{F(emp)}{F(x)}',
          lambda: f'{F(x)}{F(emp)}', lambda: f'{5:{F(T(">3"))}}', lambda: f'{"a":{F(T(">3"))}}',
          lambda: f'{1.5:{F(T(".3f"))}}', lambda: f'{5:{F(T(""))}}', lambda: f'{5:{T(">3")}}', lambda: f'{5:{T(">3")!s}}',
          lambda: f'{5:{F(T(">3"))}{""}}', lambda: f'{5:>{F(T("3"))}}', lambda: f'{G():{F(T(">3"))}}',
          lambda: f'{G():{F(T(">3"))}z}', lambda: f'{F(emp):{"s"}}',
          lambda: format(5, T('>3')), lambda: format('a', T('>3')), lambda: format(1.5, T('>5')), lambda: format(G(), T('>3')),
          lambda: format(T('ab'), T('')), lambda: format(5, T('')), lambda: (5).__format__(T('>3')),
          lambda: (1.5).__format__(T('>5')), lambda: 'ab'.__format__(T('>4')), lambda: format(F(emp)), lambda: format(F(emp), '')):
    print(show(f))


# `%` conversions (review M2): an int or float subclass is its own value
# before `__int__`/`__float__`/`__index__` (`PyLong_Check`/`PyFloat_Check`
# first), `%d` takes `__int__` before `__index__`, a `*` width or
# precision takes an int subclass, and `%c` a one-char str (a one-byte
# bytes) before `__index__`.
import enum

rows = []
class I(int): pass
class I3(int):
    def __int__(self): return 9
    def __index__(self): return 8
    def __float__(self): return 6.5
class Fl(float):
    def __float__(self): return 7.5
    def __int__(self): return 4
    def __index__(self): return 3
class FI(float):
    def __index__(self): return 3
class II(int):
    def __index__(self): return 8
class X:
    def __int__(self): return 11
    def __index__(self): return 12
    def __float__(self): return 1.25
class Xi:
    def __index__(self): return 12
class Xf:
    def __float__(self): return 1.25
class SI(str):
    def __index__(self): return 66
class E(enum.IntEnum):
    W = 4
def t(f):
    try:
        return repr(f())
    except Exception as e:
        return '%s: %s' % (type(e).__name__, e)
for fmt in ('%d', '%i', '%u', '%x', '%X', '%o', '%c', '%f', '%.1f', '%g', '%e'):
    for v in (I3(5), Fl(2.5), FI(2.5), II(65), X(), Xi(), Xf(), SI('z'), E.W, I(66)):
        rows.append('%s %s %s %s %s' % (fmt, type(v).__name__, t(lambda: fmt % v), t(lambda: fmt % (v,)), t(lambda: ('%(a)' + fmt[1:]) % {'a': v})))
        rows.append('  b %s' % t(lambda: fmt.encode() % (v,)))
for f in (lambda: '%*d' % (I(4), 5), lambda: '%*d' % (E.W, 5), lambda: '%.*f' % (I(2), 1.5), lambda: '%-*d|' % (I(4), 5),
          lambda: '%*s|' % (I(3), 'a'), lambda: b'%*d' % (I(4), 5), lambda: b'%.*f' % (E.W, 1.5), lambda: '%*d' % I(3),
          lambda: '%*d' % (Xi(), 5), lambda: '%.*f' % (2.0, 1.5), lambda: '%*d' % 3):
    rows.append(t(f))
y = '%d|%f'
y %= (I3(5), Fl(2.5))
rows.append(y)
z = '%d'
z %= I3(5)
rows.append(z)
class BI(bytes):
    def __index__(self): return 67
rows.append(' '.join([t(lambda: b'%c' % (BI(b'q'),)), t(lambda: b'%c' % BI(b'q')), t(lambda: '%c' % (SI('z'),)),
                      t(lambda: '%c' % (BI(b'q'),))]))
print('\n'.join(rows))

