# `bytes(x)` calls `x`'s class's `__bytes__` before it reads `x` as a count,
# text or items; `bytearray(x)` never does.
def t(label, f):
    try:
        r = f()
        print(label, type(r).__name__, r)
    except Exception as e:
        print(label, type(e).__name__, e)


class B:
    def __bytes__(self):
        return b'by'

    def __index__(self):
        return 3


class B2:
    def __bytes__(self):
        return b'b2'


class Bad:
    def __bytes__(self):
        return 5


class Raises:
    def __bytes__(self):
        raise ValueError('nope')


class Sub(B2):
    pass


class MB(bytes):
    pass


class RetMB:
    def __bytes__(self):
        return MB(b'mb')


class S(str):
    def __bytes__(self):
        return b'from-str'


class I(int):
    def __bytes__(self):
        return b'from-int'


class L(list):
    def __bytes__(self):
        return b'from-list'


class Own(bytes):
    def __bytes__(self):
        return b'own'


class It:
    def __bytes__(self):
        return b'it'

    def __iter__(self):
        return iter([1, 2])


t('index too', lambda: bytes(B()))
t('only', lambda: bytes(B2()))
t('non-bytes', lambda: bytes(Bad()))
t('raises', lambda: bytes(Raises()))
t('inherited', lambda: bytes(Sub()))
t('bytes subclass result', lambda: bytes(RetMB()))
t('str subclass', lambda: bytes(S('x')))
t('int subclass', lambda: bytes(I(2)))
t('list subclass', lambda: bytes(L([1])))
t('bytes subclass', lambda: bytes(Own(b'z')))
t('iterable', lambda: bytes(It()))
t('keyword', lambda: bytes(source=B2()))
t('encoding', lambda: bytes(B2(), 'utf-8'))
t('errors', lambda: bytes(B2(), errors='strict'))
t('bytearray', lambda: bytearray(B()))
t('subclass call', lambda: MB(B2()))
t('__new__', lambda: bytes.__new__(bytes, B2()))
t('from_bytes', lambda: int.from_bytes(B2(), 'big'))
t('%b', lambda: b'%b' % B2())


# No `__bytes__`, `__index__`, `__iter__` or `__getitem__`: CPython's error.
class Plain:
    pass


class Items:
    def __getitem__(self, i):
        if i < 2:
            return i + 7
        raise IndexError(i)


o = Plain()
o.__bytes__ = lambda: b'instance'
t('instance attribute', lambda: bytes(o))
t('plain', lambda: bytes(Plain()))
t('plain bytearray', lambda: bytearray(Plain()))
t('__bytes__ to bytearray', lambda: bytearray(B2()))
t('__getitem__', lambda: bytes(Items()))
t('__getitem__ bytearray', lambda: bytearray(Items()))
