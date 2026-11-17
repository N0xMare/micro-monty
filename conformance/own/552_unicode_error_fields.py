# `UnicodeDecodeError` and `UnicodeEncodeError` have CPython's five fields.
# `args` is `(encoding, object, start, end, reason)` for the codecs' own
# errors and for constructed ones. The two types' own `__init__` checks the
# arguments as CPython's does and keeps the fields, which `args` does not
# change later; `str()` is made from the fields: one byte or character
# named when `start:end` is one inside `object`, else the range, and
# nothing for an instance that `__init__` never saw. `object` is what the
# codec read: the `str` itself, a `bytes` equal to the input (of a
# `bytearray` too, copied once, and of what follows utf-8-sig's BOM).
import codecs
import copy
import sys
import traceback
import xml.etree.ElementTree as ET

D, E = UnicodeDecodeError, UnicodeEncodeError


def t(name, f):
    try:
        print(name, '=>', repr(f()))
    except BaseException as ex:
        print(name, 'RAISES', type(ex).__name__, ex)


def catch(f):
    try:
        f()
    except UnicodeError as e:
        return e


def fields(e):
    return (type(e).__name__, e.encoding, e.object, e.start, e.end, e.reason, str(e), repr(e), e.args)


def kinds(e):
    return [type(a).__name__ for a in e.args], type(e.object).__name__, e.object is e.args[1]


def bare(c):
    raise c


# Strict decode errors.
for name, data, enc in (
    ('start byte', b'ab\xff', 'utf-8'),
    ('continuation', b'a\xe9b', 'utf-8'),
    ('truncated 2 of 3', b'ab\xe2\x82', 'utf-8'),
    ('truncated 1 of 3', b'ab\xe2', 'utf-8'),
    ('truncated 3 of 4', b'ab\xf0\x9f\x98', 'utf-8'),
    ('surrogate', b'\xed\xa0\x80', 'utf-8'),
    ('overlong', b'\xc0\x80', 'utf-8'),
    ('past U+10FFFF', b'x\xf5\x80', 'utf-8'),
    ('alias UTF8', b'\xff', 'UTF8'),
    ('alias u8', b'\xff', 'U8'),
    ('ascii', b'ab\x80c', 'ascii'),
    ('alias US-ASCII', b'ab\x80c', 'US-ASCII'),
    ('utf-8-sig', b'\xef\xbb\xbfab\xff', 'utf-8-sig'),
    ('utf-8-sig no BOM', b'ab\xff', 'utf-8-sig'),
    ('utf-16 odd', b'a\x00b', 'utf-16'),
    ('utf-16-le odd', b'a\x00b', 'utf-16-le'),
    ('utf-16-be odd', b'\x00a\x00', 'utf-16-be'),
    ('utf-16 BOM odd', b'\xff\xfea\x00b', 'utf-16'),
    ('utf-16 BOM be odd', b'\xfe\xff\x00a\x00', 'utf-16'),
    ('utf-16 lone high', b'a\x00\x00\xd8b\x00', 'utf-16-le'),
    ('utf-16 lone low', b'a\x00\x00\xdcb\x00', 'utf-16-le'),
    ('utf-16 high at end', b'a\x00\x00\xd8', 'utf-16-le'),
    ('utf-32 short', b'a\x00\x00\x00b\x00', 'utf-32-le'),
    ('utf-32 range', b'a\x00\x00\x00\x00\x00\x11\x00', 'utf-32-le'),
    ('utf-32 surrogate', b'\x00\xd8\x00\x00', 'utf-32-le'),
    ('utf-32 BOM', b'\xff\xfe\x00\x00a\x00\x00\x00b\x00', 'utf-32'),
    ('utf-32-be range', b'\x00\x00\x00a\x00\x11\x00\x00', 'utf-32-be'),
):
    e = catch(lambda: data.decode(enc))
    print('decode', name, fields(e), kinds(e))
    print('str()', name, fields(catch(lambda: str(data, enc))))
    e = catch(lambda: bytearray(data).decode(enc))
    print('bytearray', name, fields(e), kinds(e))
print('codecs.decode', fields(catch(lambda: codecs.decode(b'ab\xff', 'utf-8'))))
print('latin-1 decodes', b'\xff'.decode('latin-1') == '\xff')

# `object` of a `bytearray` is a `bytes` of it as it was.
ba = bytearray(b'ab\xffcd')
e = catch(lambda: ba.decode())
ba[0] = 0x7a
print('bytearray after', e.object, e.args[1], ba)

# Strict encode errors: `object` is the `str` itself.
for name, s, enc in (
    ('ascii one', 'ab\xe9', 'ascii'),
    ('ascii run', 'ab\xe9\xe8\u20acz', 'ascii'),
    ('ascii two runs', 'a\xe9b\xe8\xe7', 'ascii'),
    ('ascii BMP', '\u20ac', 'ascii'),
    ('ascii astral', 'a\U0001f600', 'ascii'),
    ('ascii run to the end', 'ab\xe9\xe8', 'ascii'),
    ('latin-1 one', 'ab\u20ac', 'latin-1'),
    ('latin-1 run', 'ab\u20ac\u20adz', 'latin-1'),
    ('alias iso-8859-1', 'ab\u20ac', 'iso-8859-1'),
):
    e = catch(lambda: s.encode(enc))
    print('encode', name, fields(e), kinds(e), e.object is s)
    print('bytes()', name, fields(catch(lambda: bytes(s, enc))))
    print('bytearray()', name, fields(catch(lambda: bytearray(s, enc))))
print('codecs.encode', fields(catch(lambda: codecs.encode('ab\xe9', 'ascii'))))

# Handlers that do not raise, and the ones that cannot handle the error.
print(b'ab\xffcd'.decode('utf-8', 'replace'), b'ab\xffcd'.decode('utf-8', 'ignore'),
      b'ab\xffcd'.decode('utf-8', 'backslashreplace'))
print('ab\xe9cd'.encode('ascii', 'replace'), 'ab\xe9cd'.encode('ascii', 'ignore'),
      'ab\xe9cd'.encode('ascii', 'backslashreplace'), 'ab\xe9cd'.encode('ascii', 'xmlcharrefreplace'),
      'ab\xe9cd'.encode('ascii', 'namereplace'))
t('xmlcharrefreplace decoding', lambda: b'\xff'.decode('utf-8', 'xmlcharrefreplace'))
t('unknown handler decoding', lambda: b'\xff'.decode('utf-8', 'nope'))
t('unknown handler encoding', lambda: '\xe9'.encode('ascii', 'nope'))

# An error far into a large input, past several steps of the codec.
big = b'a' * 1_200_000 + b'\xe2\x82'
e = catch(lambda: big.decode())
print('large decode', e.start, e.end, e.reason, len(e.object), e.object == big, str(e))
print('large decode prefix', len(big[:e.start].decode()), big[e.start:e.end])
big = bytearray(b'a' * 700_000 + b'\xff' + b'b' * 300_000)
e = catch(lambda: big.decode('ascii'))
print('large bytearray', e.start, e.end, type(e.object).__name__, e.object == big, str(e))
big = '\xe9' * 600_000 + 'a\u20ac\u20adb' + 'c' * 100_000
e = catch(lambda: big.encode('latin-1'))
print('large encode', e.start, e.end, e.reason, len(e.object), e.object is big, str(e))
print('large encode slice', big[e.start:e.end], big[e.end:e.end + 2])
e = catch(lambda: ('\xe9' * 600_000 + '\u20ac').encode('latin-1'))
print('large encode one', e.start, e.end, str(e))

# A `bytes` of a `bytearray`, and of what follows a BOM, past one step's copy.
big = bytearray(b'a' * 9_000_000 + b'\xff' + b'b' * 5)
e = catch(lambda: big.decode())
big[0] = 0x7a
print('stepped copy', type(e.object).__name__, len(e.object), e.object[:2], e.object[-7:], e.start, e.end,
      e.object is e.args[1], str(e))
e = D('u', big, 1, 2, 'r')
print('stepped object', type(e.object).__name__, len(e.object), e.object[:2], type(e.args[1]).__name__, str(e))
big = b'\xef\xbb\xbf' + b'a' * 9_000_000 + b'\xe2\x82'
e = catch(lambda: big.decode('utf-8-sig'))
print('stepped copy after a BOM', type(e.object).__name__, len(e.object), e.object[:2], e.object[-3:], e.start, e.end,
      str(e))
del big, e

# Constructed.
t('D', lambda: fields(D('utf-8', b'ab\xff', 2, 3, 'why')))
t('E', lambda: fields(E('ascii', 'ab\xe9', 2, 3, 'why')))
t('D bools', lambda: fields(D('u', b'x', False, True, 'r')))
t('D bytearray', lambda: fields(D('u', bytearray(b'xy'), 0, 1, 'r')))
for cls in (D, E):
    n = cls.__name__
    ok = b'x' if cls is D else 'x'
    t(n + ' no arguments', lambda: cls())
    t(n + ' one', lambda: cls('x'))
    t(n + ' four', lambda: cls('u', ok, 0, 1))
    t(n + ' six', lambda: cls('u', ok, 0, 1, 'r', 6))
    t(n + ' keyword', lambda: cls('u', ok, 0, 1, reason='r'))
    t(n + ' keywords only', lambda: cls(encoding='u'))
    t(n + ' encoding int', lambda: cls(1, ok, 0, 1, 'r'))
    t(n + ' encoding bytes', lambda: cls(b'u', ok, 0, 1, 'r'))
    t(n + ' encoding None', lambda: cls(None, ok, 0, 1, 'r'))
    t(n + ' object int', lambda: cls('u', 5, 0, 1, 'r'))
    t(n + ' object None', lambda: cls('u', None, 0, 1, 'r'))
    t(n + ' object list', lambda: cls('u', [1], 0, 1, 'r'))
    t(n + ' object of the other', lambda: cls('u', 'x' if cls is D else b'x', 0, 1, 'r'))
    t(n + ' start str', lambda: cls('u', ok, '0', 1, 'r'))
    t(n + ' start float', lambda: cls('u', ok, 0.0, 1, 'r'))
    t(n + ' start None', lambda: cls('u', ok, None, 1, 'r'))
    t(n + ' start big', lambda: cls('u', ok, 10 ** 30, 1, 'r'))
    t(n + ' end float', lambda: cls('u', ok, 0, 1.5, 'r'))
    t(n + ' end big', lambda: cls('u', ok, 0, -10 ** 30, 'r'))
    t(n + ' reason int', lambda: cls('u', ok, 0, 1, 5))
    t(n + ' reason bytes', lambda: cls('u', ok, 0, 1, b'r'))
    t(n + ' reason None', lambda: cls('u', ok, 0, 1, None))
    t(n + ' all bad', lambda: cls(1, 2, 3, 4, 5))
    t(n + ' object and start bad', lambda: cls('u', 2, 'x', 4, 'r'))
    t(n + ' object and reason bad', lambda: cls('u', 2, 3, 4, 5))
    t(n + ' raised bare', lambda: bare(cls))
    t(n + ' thrown bare', lambda: (_ for _ in ()).throw(cls))

# `str()` from the fields.
for name, a in (
    ('range', (1, 4)), ('one', (1, 2)), ('first', (0, 1)), ('last', (5, 6)), ('empty range', (2, 2)),
    ('negative', (-1, 0)), ('past the end', (10, 11)), ('end past', (5, 60)), ('end before', (4, 1)),
    ('at the end', (6, 7)), ('negative end', (1, -1)), ('large', (2 ** 62, 2 ** 62 + 1)),
):
    print('D', name, str(D('utf-8', b'abc\x00\x0af', a[0], a[1], 'why')))
    print('E', name, str(E('ascii', 'abc\x00\xe9\u20ac', a[0], a[1], 'why')))
print(str(D('utf-8', b'', 0, 1, 'why')), '|', str(D('utf-8', b'', 0, 0, 'why')), '|',
      str(E('ascii', '', 0, 1, 'why')), '|', str(E('ascii', '', 0, 0, 'why')))
print(str(E('a', "'\n\U0001f600\xff\u0100", 0, 1, 'r')), str(E('a', "'\n\U0001f600\xff\u0100", 1, 2, 'r')),
      str(E('a', "'\n\U0001f600\xff\u0100", 2, 3, 'r')), str(E('a', "'\n\U0001f600\xff\u0100", 3, 4, 'r')),
      str(E('a', "'\n\U0001f600\xff\u0100", 4, 5, 'r')))
print(repr(str(E('a\nb', 'abc', 0, 1, 'w\ny'))), repr(str(D('', b'abc', 0, 1, ''))))
e = D('utf-8', b'ab\xff', 2, 3, 'why')
print(str(e), '|', '%s' % e, '|', f'{e}', '|', format(e), '|', e.__str__(), '|', repr(e), '|', '%r' % (e,))
print(isinstance(e, ValueError), isinstance(e, UnicodeError), issubclass(D, ValueError), issubclass(E, UnicodeError),
      isinstance(E('a', 'b', 0, 1, 'r'), D), D.__mro__ == (D, UnicodeError, ValueError, Exception, BaseException, object))
print(sorted(vars(e)), e.__cause__, e.__context__, getattr(e, 'start', 'none'), getattr(e, 'nope', 'none'))
print(hasattr(ValueError(), 'start'), hasattr(UnicodeError('a', b'b', 1, 2, 'r'), 'reason'),
      hasattr(OSError(), 'encoding'), UnicodeError('a', 'b').args, str(UnicodeError()))


# An assigned field comes first, in the attribute and in `str()`; `args`
# and `repr()` keep the arguments.
def assigned(e, k, v):
    setattr(e, k, v)
    return fields(e)


def both(e):
    e.start = 1
    e.end = 2
    return fields(e)


for name, mk, obj in (('D', lambda: D('utf-8', b'ab\xffcd', 2, 3, 'why'), b'xyz'),
                      ('E', lambda: E('ascii', 'ab\xe9cd', 2, 3, 'why'), 'xy\u20ac')):
    t(name + ' start', lambda: assigned(mk(), 'start', 0))
    t(name + ' start past', lambda: assigned(mk(), 'start', 99))
    t(name + ' start negative', lambda: assigned(mk(), 'start', -5))
    t(name + ' end', lambda: assigned(mk(), 'end', 5))
    t(name + ' reason', lambda: assigned(mk(), 'reason', 'new'))
    t(name + ' reason None', lambda: assigned(mk(), 'reason', None))
    t(name + ' reason int', lambda: assigned(mk(), 'reason', 5))
    t(name + ' encoding', lambda: assigned(mk(), 'encoding', 'latin-1'))
    t(name + ' encoding int', lambda: assigned(mk(), 'encoding', 5))
    t(name + ' object', lambda: assigned(mk(), 'object', obj))
    t(name + ' both', lambda: both(mk()))
real = catch(lambda: b'ab\xffcd'.decode())
real.start, real.end, real.reason = 0, 2, 'moved'
print('real assigned', fields(real))
real = catch(lambda: 'ab\xe9cd'.encode('ascii'))
real.start = 3
print('real assigned', fields(real))


# An `encoding` or `reason` that is not a `str` is shown by its `str()`.
class Shown:
    def __str__(self):
        return 'shown'


class OwnText(str):
    def __str__(self):
        return 'own'


def shown(e, k, v):
    setattr(e, k, v)
    return str(e)


def itself():
    e = D('u', b'ab', 0, 1, 'r')
    e.reason = e
    try:
        return str(e)
    except RecursionError:
        return 'RecursionError'


t('reason object', lambda: shown(D('u', b'ab', 0, 1, 'r'), 'reason', Shown()))
t('encoding object', lambda: shown(E('u', 'ab', 0, 1, 'r'), 'encoding', Shown()))
t('reason exception', lambda: shown(D('u', b'ab', 0, 2, 'r'), 'reason', ValueError('inner')))
t('reason a codec error', lambda: shown(E('u', 'ab', 0, 1, 'r'), 'reason', D('v', b'cd', 1, 2, 'in')))
t('str subclasses', lambda: fields(D(OwnText('u'), b'ab', 0, 1, OwnText('r')))[:7])
t('reason itself', itself)
try:
    e = E('u', 'ab\xe9', 2, 3, 'r')
    e.reason = Shown()
    raise e
except E as e:
    traceback.print_exception(e, file=sys.stdout)


def extra(e):
    e.extra = [1]
    e.add_note('note')
    return sorted(vars(e)), e.__notes__, fields(e.with_traceback(None))


t('other attributes', lambda: extra(D('utf-8', b'ab\xff', 2, 3, 'why')))
t('__init__ again', lambda: (lambda e: (e.__init__('latin-1', b'q', 0, 1, 'again'), fields(e)))(D('utf-8', b'ab', 0, 1, 'r')))
t('__init__ again bad', lambda: D('utf-8', b'ab', 0, 1, 'r').__init__('latin-1'))


# Subclasses.
class SD(D):
    pass


class SE(E):
    pass


class WithInit(D):
    def __init__(self, data, at):
        super().__init__('mine', data, at, at + 1, 'custom')
        self.data = data


class Before(E):
    def __init__(self, *a):
        self.before = 1
        super().__init__(*a)


class NoSuper(D):
    def __init__(self, *a):
        self.seen = a


class NoSuperE(E):
    def __init__(self, *a):
        pass


class OwnStr(D):
    def __str__(self):
        return 'OwnStr<' + super().__str__() + '>'


class Deeper(SD):
    pass


t('SD', lambda: fields(SD('utf-8', b'ab\xff', 2, 3, 'why')))
t('SE', lambda: fields(SE('ascii', 'ab\xe9', 2, 3, 'why')))
t('Deeper', lambda: fields(Deeper('utf-8', b'ab\xff', 0, 2, 'why')))
t('SD no arguments', lambda: SD())
t('SD one', lambda: SD('x'))
t('SD object str', lambda: SD('utf-8', 'ab', 2, 3, 'why'))
t('SE start str', lambda: SE('ascii', 'ab', '2', 3, 'why'))
t('SD keyword', lambda: SD('utf-8', b'ab', 2, 3, reason='why'))
t('Deeper reason int', lambda: Deeper('utf-8', b'ab', 2, 3, 4))
t('SD raised bare', lambda: bare(SD))
t('WithInit', lambda: (lambda e: (fields(e), e.data))(WithInit(b'abc', 1)))
t('WithInit bad', lambda: WithInit('abc', 1))
t('Before', lambda: (lambda e: (fields(e), e.before))(Before('ascii', 'ab\xe9', 2, 3, 'why')))
t('Before bad', lambda: Before('ascii'))
t('NoSuper', lambda: (lambda e: (fields(e), e.seen))(NoSuper('x')))
t('NoSuper none', lambda: fields(NoSuper()))
t('NoSuperE', lambda: fields(NoSuperE('x', 2)))
t('OwnStr', lambda: fields(OwnStr('utf-8', b'ab\xff', 2, 3, 'why')))
t('isinstance', lambda: (isinstance(SD('u', b'', 0, 0, 'r'), D), isinstance(SE('u', '', 0, 0, 'r'), ValueError)))


# Copies read the arguments again.
def copied(c, e):
    x = c(e)
    return fields(x), x is e, x.args == e.args, x.object == e.object


for how, c in (('copy', copy.copy), ('deepcopy', copy.deepcopy)):
    t(how + ' real D', lambda: copied(c, catch(lambda: b'ab\xff'.decode())))
    t(how + ' real D bytearray', lambda: copied(c, catch(lambda: bytearray(b'ab\xe2\x82').decode())))
    t(how + ' real E', lambda: copied(c, catch(lambda: 'ab\xe9\xe8'.encode('ascii'))))
    t(how + ' D', lambda: copied(c, D('utf-8', b'ab\xffcd', 2, 3, 'why')))
    t(how + ' E', lambda: copied(c, E('ascii', 'ab\xe9cd', 2, 3, 'why')))
    t(how + ' SD', lambda: copied(c, SD('utf-8', b'ab\xff', 2, 3, 'why')))
    t(how + ' WithInit', lambda: copied(c, WithInit(b'abc', 1)))
    t(how + ' extra', lambda: (lambda e: (setattr(e, 'extra', [1]), vars(c(e)))[1])(D('u', b'ab', 0, 1, 'r')))
    t(how + ' in a list', lambda: (lambda e, l: (l[0] is l[1], l[0] is e, fields(l[0])))(
        *(lambda e: (e, c([e, e])))(catch(lambda: '\u20ac'.encode('latin-1')))))

# The codecs module's handlers read `object`, `start` and `end`.
real_d = catch(lambda: b'ab\xff\xfecd'.decode())
real_e = catch(lambda: 'ab\xe9\u20accd'.encode('ascii'))
for h in ('ignore', 'replace', 'backslashreplace', 'xmlcharrefreplace', 'namereplace', 'strict'):
    f = codecs.lookup_error(h)
    t(h + ' real D', lambda: f(real_d))
    t(h + ' real E', lambda: f(real_e))
    t(h + ' D', lambda: f(D('utf-8', b'ab\xffcd', 2, 4, 'why')))
    t(h + ' E', lambda: f(E('ascii', 'ab\xe9€cd', 2, 4, 'why')))


# What code that handles a tool's bytes writes.
def salvage(data):
    try:
        return data.decode('utf-8')
    except UnicodeDecodeError as e:
        return data[:e.start].decode('utf-8') + '<' + data[e.start:e.end].hex() + '>' + repr(data[e.end:])


def salvage_all(data):
    out = []
    while data:
        try:
            out.append(data.decode('utf-8'))
            break
        except UnicodeDecodeError as e:
            out.append(data[:e.start].decode('utf-8'))
            out.append('\\x' + e.object[e.start:e.end].hex())
            data = data[e.end:]
    return out


def to_ascii(s):
    try:
        return s.encode('ascii')
    except UnicodeEncodeError as e:
        return s[:e.start], s[e.start:e.end], s[e.end:], e.encoding, e.reason


print(salvage(b'caf\xc3\xa9 \xff tail'), salvage(bytearray(b'ok\xe2\x82')), salvage(b'fine'))
print(salvage_all(b'a\xffb\xfe\xe2\x82c\xe2\x82'))
print(to_ascii('abc\xe9\xe8def'), to_ascii('plain'))
try:
    raise D('utf-8', b'ab\xff', 2, 3, 'why')
except ValueError as v:
    print('as ValueError', v.start, v.end, v.object[v.start:v.end], v)
for data in (b'<a>\xff</a>', b'<?xml version="1.0" encoding="ascii"?><a>caf\xc3\xa9</a>', b'<a>\n<b>x\xe2\x82</b></a>'):
    try:
        print(ET.fromstring(data).tag)
    except ET.ParseError as e:
        print('ParseError', e, e.code, e.position)

# The fields are kept when `__init__` runs. `args` assigned later, or an
# `__init__` chain that never reaches the Unicode type's, leaves them as
# they are; the members check what is assigned, and `start` and `end`
# cannot be deleted.
class ViaException(D):
    def __init__(self, msg):
        Exception.__init__(self, msg)


class ViaValueError(E):
    def __init__(self, msg):
        ValueError.__init__(self, msg)


class ViaUnicodeError(D):
    def __init__(self, msg):
        UnicodeError.__init__(self, msg)


class ViaSuper(D):
    def __init__(self, *a):
        super().__init__(*a)


class StrViaBase(D):
    def __str__(self):
        return BaseException.__str__(self) + ' | ' + super().__str__()


for cls in (ViaException, ViaValueError, ViaUnicodeError):
    t('init via ' + cls.__name__, lambda: fields(cls('hello')))
t('init via super', lambda: fields(ViaSuper('u', b'q\xff', 1, 2, 'r')))
t('init via super, one', lambda: fields(ViaSuper('m')))
t('BaseException.__str__', lambda: str(StrViaBase('u', b'q\xff', 1, 2, 'r')))


def args_after(new):
    e = catch(lambda: b'ab\xff'.decode())
    e.args = new
    return fields(e), vars(e)


t('args = (msg,)', lambda: args_after(("bad data in f.txt",)))
t('args = ()', lambda: args_after(()))
t('args = other five', lambda: args_after(('x', b'zz', 0, 1, 'y')))
t('copy after args', lambda: fields(copy.copy(catch(lambda: b'ab\xff'.decode()))))


def reinit(*a):
    e = D('a', b'xyz', 0, 1, 'r')
    try:
        e.__init__(*a)
    except TypeError as x:
        print('  reinit raised', x)
    return fields(e)


t('reinit', lambda: reinit('b', b'q', 0, 2, 's'))
t('reinit, one', lambda: reinit('b'))
t('E.__init__ on a D', lambda: E.__init__(D('a', b'x', 0, 1, 'r'), 'a', 'x', 0, 1, 'r'))


# `BaseException`'s `__init__` and `__str__` called through a type on one
# of the two (not a subclass): the arguments only, the fields kept.
def annotate(*a):
    e = catch(lambda: b'ab\xff'.decode())
    Exception.__init__(e, *a)
    return fields(e), BaseException.__str__(e)


t('annotate a caught error', lambda: annotate('while reading f.txt'))
t('annotate with five', lambda: annotate('x', b'zz', 0, 1, 'y'))
t('BaseException.__init__ on an encode error', lambda: (lambda e: (BaseException.__init__(e), fields(e), ValueError.__str__(e)))(E('a', 'x', 0, 1, 'r')))
t('super past the type', lambda: (lambda e: (super(D, e).__init__('m'), fields(e)))(D('a', b'x', 0, 1, 'r')))


# A subclass with `__new__` and no `__init__`: the type's `__init__` runs
# on the call's arguments, converting an `__index__` or a `bytearray`.
class NewOnly(D):
    def __new__(cls, *a):
        return super().__new__(cls, *a)


class NewOnlyE(E):
    def __new__(cls, *a):
        return E.__new__(cls, *a)


class Two:
    def __index__(self):
        return 2

    def __repr__(self):
        return 'Two()'


t('new only', lambda: fields(NewOnly('u', b'ab\xff', 2, 3, 'r')))
t('new only, index', lambda: fields(NewOnly('u', b'ab\xff', Two(), 3, 'r')))
t('new only, bytearray', lambda: fields(NewOnly('u', bytearray(b'ab\xff'), 2, 3, 'r')))
t('new only, encode index', lambda: fields(NewOnlyE('u', 'ab\xe9', 1, Two(), 'r')))
t('new only, short', lambda: fields(NewOnly('m')))


class Index:
    def __index__(self):
        return 1

    def __repr__(self):
        return 'Index()'


t('start and end by __index__', lambda: fields(D('u', b'abc', Index(), 2, 'r')))
t('encode start by __index__', lambda: fields(E('u', 'abc', Index(), Index(), 'r')))


def assign(name, v, enc=False):
    e = catch(lambda: 'a\xe9'.encode('ascii')) if enc else catch(lambda: b'ab\xff'.decode())
    setattr(e, name, v)
    try:
        s = str(e)
    except TypeError as x:
        s = 'str RAISES ' + str(x)
    return getattr(e, name), s, vars(e)


for name, vals in (
    ('start', [5, -1, True, 'x', None, 2**70, Index(), 1.5]),
    ('end', [9, 'x']),
    ('encoding', ['zz', 5, None]),
    ('reason', ['why', 5]),
    ('object', [b'qq\xffr', bytearray(b'q'), 'str', None]),
):
    for v in vals:
        t('assign %s=%r' % (name, v), lambda: assign(name, v))
        if name == 'object':
            t('assign encode %s=%r' % (name, v), lambda: assign(name, v, True))


def remove(name):
    e = catch(lambda: b'ab\xff'.decode())
    delattr(e, name)
    return fields(e)


for name in ('encoding', 'object', 'start', 'end', 'reason'):
    t('del ' + name, lambda: remove(name))


# Constructed with a `bytearray`, `object` is a `bytes` copied once: later
# changes to the bytearray do not show, and each read is the same object.
def from_bytearray():
    ba = bytearray(b'abc\xff')
    e = D('utf-8', ba, 3, 4, 'r')
    first = e.object
    ba[3] = 0x41
    del ba[1:]
    return fields(e), e.object is first, type(e.args[1]).__name__


t('from a bytearray', from_bytearray)

# str() takes `reason`'s and `encoding`'s str() first, in that order, then
# checks `object`, and uses the strings it got.
class Loud:
    def __init__(self, n, then=None):
        self.n, self.then = n, then

    def __str__(self):
        print('str of', self.n)
        if self.then:
            self.then()
        return self.n


def str_order():
    e = UnicodeDecodeError('utf-8', b'x', 0, 1, 'r')
    e.reason, e.encoding, e.object = Loud('REASON'), Loud('ENC'), 5
    return str(e)


t('str order, bad object', str_order)


def str_repair():
    e = UnicodeDecodeError('utf-8', b'x', 0, 1, 'r')
    e.object = 5

    def fix():
        e.object = b'ab'
    e.reason = Loud('fixed', fix)
    return str(e)


t('str repairs object', str_repair)


def str_gone():
    e = UnicodeEncodeError('ascii', 'x', 0, 1, 'r')

    def gone():
        del e.reason
    e.reason = Loud('gone', gone)
    return str(e), e.reason


t('str deletes reason', str_gone)


def init_keywords():
    e = UnicodeDecodeError('utf-8', b'ab\xff', 2, 3, 'why')
    out = []
    for f in (lambda: e.__init__('ascii', b'q', 0, 1, 'new', k=1),
              lambda: e.__init__(encoding='a'),
              lambda: SD('utf-8', b'a', 0, 1, 'r', k=1)):
        try:
            out.append(f())
        except TypeError as x:
            out.append(str(x))
    return out, e.args, e.encoding


t('init keywords', init_keywords)


class ClassStart(UnicodeDecodeError):
    start = 'class start'


def class_attr():
    e = ClassStart('utf-8', b'ab\xff', 1, 2, 'why')
    before = e.start
    e.start = 0
    after = (e.start, vars(e), str(e))
    del e.start
    return before, after, e.start


t('class attribute named like a field', class_attr)


def type_dict_twins():
    try:
        b'ab\xff'.decode()
    except UnicodeDecodeError as e:
        s = BaseException.__dict__['__str__'](e)
        vars(BaseException)['__init__'](e, 'msg')
        return s, e.args, e.start, str(e)


t('BaseException.__dict__ twins', type_dict_twins)


class IndexInt:
    def __index__(self):
        return 1

    def __int__(self):
        return 7


t('start with __index__ and __int__',
  lambda: UnicodeEncodeError('ascii', 'abc', IndexInt(), 2, 'r').start)

# Tracebacks: a chained one printed, then an uncaught one.
try:
    try:
        b'\xff'.decode()
    except UnicodeDecodeError as e:
        raise RuntimeError('wrapped') from e
except RuntimeError as r:
    print(type(r.__cause__).__name__, r.__cause__.start, r.__cause__.reason)
    traceback.print_exception(r, file=sys.stdout)
try:
    raise SE('ascii', 'ab\xe9\xe8', 2, 4, 'both')
except SE as e:
    traceback.print_exception(e, file=sys.stdout)


def read(data):
    return data.decode('ascii')


read(b'tool output \xff')
