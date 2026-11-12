# bytes: literals, repr, indexing, slicing, operators, methods, codecs.


def attempt(f):
    try:
        print(repr(f()))
    except Exception as e:
        print(type(e).__name__, e)


# literals and repr
print(b'', b'abc', b"it's", b'say "hi"', b'both \' and "', b'\x00\x7f\x80\xff')
print(b'\t\n\r\\', rb'raw\n', b'\x41\102C', repr(b'\'"'))
print(bytes(3), bytes([1, 2, 255]), bytes(range(65, 70)), bytes(b'copy'))
print(bytes('héllo', 'utf-8'), bytes('abc', encoding='ascii'), bytes())
attempt(lambda: bytes([256]))
attempt(lambda: bytes([-1]))
attempt(lambda: bytes(-1))
attempt(lambda: bytes('abc'))
attempt(lambda: bytes(1.5))
attempt(lambda: bytes(['a']))

# indexing, slicing, iteration
b = b'hello world'
print(b[0], b[-1], b[1:4], b[::-1], b[::2], b[100:], list(b[:3]))
attempt(lambda: b[11])
attempt(lambda: b[-12])
print([x for x in b'ab'], tuple(b'\x00\xff'), len(b), len(b''))

# operators
print(b'ab' + b'cd', b'ab' * 3, 3 * b'x', b'ab' * 0, b'ab' * -2)
print(b'ab' == b'ab', b'ab' != b'ac', b'ab' < b'abc', b'b' > b'abc', b'' <= b'')
print(b'a' == 'a', b'ell' in b, 101 in b, 300 in b'x', b'' in b)
attempt(lambda: b'a' + 'a')
attempt(lambda: 'a' in b'a')
attempt(lambda: b'a' < 'a')
print(hash(b'abc') == hash(b'abc'), {b'k': 1}[b'k'], sorted([b'b', b'a', b'']))

# methods
print(b.find(b'o'), b.find(b'o', 5), b.rfind(b'o'), b.find(b'z'), b.find(111))
print(b.index(b'w'), b.count(b'l'), b.count(b'l', 4), b.count(b''))
attempt(lambda: b.index(b'z'))
print(b.startswith(b'he'), b.endswith((b'x', b'ld')), b.startswith(b'lo', 3))
print(b.split(), b.split(b'o'), b'a,b,,c'.split(b',', 2), b.rsplit(b' ', 1))
print(b'  pad  '.strip(), b'xxhixx'.strip(b'x'), b'  l'.lstrip(), b'r  '.rstrip())
print(b'-'.join([b'a', b'b', b'c']), b''.join([]), b.replace(b'l', b'L'), b.replace(b'l', b'', 2))
print(b.upper(), b'MiXeD'.lower(), b'MiXeD'.swapcase(), b'hello world'.title(), b'abc'.capitalize())
print(b'abc'.isalpha(), b'123'.isdigit(), b'a1'.isalnum(), b' \t'.isspace(), b'ABC'.isupper(), b'abc'.islower())
print(b'ab'.center(6, b'*'), b'ab'.ljust(4), b'ab'.rjust(4, b'0'), b'42'.zfill(5), b'-42'.zfill(5))
print(b'a=b=c'.partition(b'='), b'a=b=c'.rpartition(b'='), b'abc'.partition(b'x'))
print(b'line1\nline2\r\nline3'.splitlines(), b'x\ny\n'.splitlines(True))
print(b'prefix-body'.removeprefix(b'prefix-'), b'body.txt'.removesuffix(b'.txt'))
print(b'a\tb'.expandtabs(4), b'abc'.isascii(), b'\xff'.isascii())

# hex and fromhex
print(b'\x00\xab\xff'.hex(), b'abc'.hex(':'), b'abcdef'.hex('-', 2), b''.hex())
print(bytes.fromhex('00ab ff'), bytes.fromhex(''), bytes.fromhex(' 41 42 '))
attempt(lambda: bytes.fromhex('0g'))
attempt(lambda: bytes.fromhex('abc'))

# codecs
s = 'añ€😀'
for enc in ['utf-8', 'ascii', 'latin-1']:
    for errors in ['strict', 'replace', 'ignore', 'backslashreplace', 'xmlcharrefreplace']:
        attempt(lambda: s.encode(enc, errors))
for data in [b'ok', b'caf\xc3\xa9', b'bad\xff\xfeend', b'\xe2\x82', b'\xf0\x9f\x98\x80']:
    for enc in ['utf-8', 'ascii', 'latin-1']:
        for errors in ['strict', 'replace', 'ignore', 'backslashreplace']:
            attempt(lambda: data.decode(enc, errors))
attempt(lambda: b'x'.decode('rot13'))
print(str(b'abc'), str(b'caf\xc3\xa9', 'utf-8'), 'x'.encode())
