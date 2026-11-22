# A builtin method's argument given by name and by position: the message
# names the method bare and counts positions after the receiver, as
# CPython's argument clinic does. A function keeps its name and count.
def t(f):
    try:
        f()
    except TypeError as e:
        print(e)


t(lambda: 'a'.encode(42, encoding='x'))
t(lambda: 'a'.encode('utf-8', 'strict', errors='x'))
t(lambda: 'a'.split('x', sep='y'))
t(lambda: 'a'.split('x', 1, maxsplit=2))
t(lambda: 'a'.rsplit('x', sep='y'))
t(lambda: 'a'.splitlines(True, keepends=False))
t(lambda: 'a'.expandtabs(4, tabsize=8))
t(lambda: b'a'.decode('utf-8', encoding='x'))
t(lambda: bytearray(b'a').decode('utf-8', encoding='x'))
t(lambda: (1).to_bytes(1, length=2))
t(lambda: int.from_bytes(b'a', 'big', byteorder='big'))
t(lambda: sorted([1], [2], key=int))
t(lambda: int('1', 2, base=3))
t(lambda: sum([1], 2, start=3))
t(lambda: enumerate([], 1, start=2))
t(lambda: round(1.5, 1, ndigits=2))


# A method that takes keyword-only arguments alone, given a positional one.
for call in (lambda: [3].sort(None), lambda: [3].sort(None, key=None), lambda: [3].sort(1, 2)):
    try:
        call()
    except TypeError as e:
        print(e)
