# printf-style formatting with values only Python code can convert: user
# __str__/__repr__/__index__/__int__/__float__/__bytes__, user mappings,
# containers holding user objects, and %= on a str.
class S:
    def __str__(self):
        return 'S!'

    def __repr__(self):
        return 'R\xe9!'


class I:
    def __index__(self):
        return 65


class F:
    def __float__(self):
        return 2.5


class N:
    def __int__(self):
        return 7


class B:
    def __bytes__(self):
        return b'BY'


class M:
    def __getitem__(self, k):
        return k * 2


class Bad:
    def __bytes__(self):
        return 'not bytes'


print('%s %r %a %d %x %c %f %.1f %s' % (S(), S(), S(), I(), I(), I(), F(), I(), [S()]))
print('%d' % N(), '%(ab)s|%(c)r' % M(), b'%s %b %a %c' % (B(), B(), S(), I()))
print('%-6s|%6r|%-*d|%0*.*f' % (S(), S(), 4, I(), 8, 2, F()))
print('%s and %s' % ({'k': S()}, (S(),)), '%(x)s' % {'x': S()})
for t, a in [('%x', N()), ('%c', F()), (b'%s', S()), ('%f', N()), (b'%s', Bad()),
             ('%(a)s', M()), ('%s %s', S()), ('%d', S())]:
    try:
        print(t % a)
    except Exception as e:
        print(type(e).__name__, e)
x = 'abc %s %s'
x %= (S(), 5)
print(x)
y = b'%c%c'
y %= (I(), 66)
print(y)
