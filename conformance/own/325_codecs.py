# codecs: lookup, encode/decode, search functions and error handlers (docs/spec.md §12).
import codecs

for n in ['utf-8', 'UTF8', 'u8', 'ascii', 'US-ASCII', 'latin-1', 'latin1', 'iso-8859-1', 'utf-16',
          'utf_16le', 'UTF-16-BE', 'u32', 'utf-32-le', 'utf-32-be']:
    ci = codecs.lookup(n)
    print(n, ci.name, isinstance(ci, tuple), len(ci), ci[0] is ci.encode, ci[1] is ci.decode)
    s = 'ab' if ci.name == 'ascii' else 'aé'
    print(' ', ci.encode(s), ci.decode(ci.encode(s)[0]), ci.encode(s, 'strict') == ci.encode(s))
print(codecs.lookup('utf-8') is codecs.lookup('UTF_8'))
print(codecs.encode('héllo'), codecs.encode('héllo', 'latin-1'), codecs.decode(b'h\xc3\xa9'))
print(codecs.decode(b'\xff\xfeh\x00i\x00', 'utf-16'), codecs.encode('hi', 'utf-16-be'))
print(codecs.encode('aé€', 'ascii', 'replace'), codecs.encode('aé', 'ascii', 'xmlcharrefreplace'))
print(codecs.encode('aé', 'ascii', 'backslashreplace'), codecs.decode(b'a\xffb', 'utf-8', 'replace'))
print(codecs.decode(b'a\xffb', 'ascii', 'ignore'), codecs.getencoder('ascii')('abc'), codecs.getdecoder('latin-1')(b'\xe9'))
print(codecs.encode(errors='strict', obj='x', encoding='ascii'))
for f, args in [(codecs.encode, ('aé', 'ascii')), (codecs.decode, (b'\xff',)), (codecs.lookup, ('no-such-codec',)),
                (codecs.encode, (b'x',)), (codecs.encode, (1,)), (codecs.decode, ('x',)), (codecs.lookup, (1,)),
                (codecs.register, (1,)), (codecs.lookup_error, ('nope',)), (codecs.register_error, ('x', 1)),
                (codecs.getencoder, ('cp-none',))]:
    try:
        print(f(*args))
    except (TypeError, LookupError, UnicodeError) as e:
        print(type(e).__name__, e)

print(codecs.BOM_UTF8, codecs.BOM_UTF16_LE, codecs.BOM_UTF16_BE, codecs.BOM_UTF32_LE, codecs.BOM_UTF32_BE)
print(codecs.BOM == codecs.BOM_UTF16 == codecs.BOM_LE, codecs.BOM_UTF32 == codecs.BOM64_LE, codecs.BOM_BE == codecs.BOM32_BE)
print('x'.encode('utf-16').startswith(codecs.BOM_UTF16), 'x'.encode('utf-32').startswith(codecs.BOM_UTF32))

# Search functions: consulted after the built-in codecs, normalized names, cached.
seen = []
def search(name):
    seen.append(name)
    if name == 'rot_x':
        return codecs.CodecInfo(lambda s, errors='strict': (s[::-1].encode(), len(s)),
                                lambda b, errors='strict': (bytes(b).decode()[::-1], len(b)), name='rot-x')
    if name == 'four':
        return (lambda s, e='strict': (s.upper(), len(s)), lambda b, e='strict': 42, None, None)
    if name == 'bad':
        return (1, 2, 3)
    if name == 'num':
        return 42
    return None
codecs.register(search)
ci = codecs.lookup('Rot X')
print(ci.name, codecs.lookup('rot-x') is ci, codecs.encode('abc', 'ROT--X'), codecs.decode(b'cba', ' rot x '))
print(codecs.encode('abc', 'four'), codecs.lookup('four')[2:])
for n in ['bad', 'num', 'unknown-thing']:
    try:
        codecs.lookup(n)
    except (TypeError, LookupError) as e:
        print(type(e).__name__, e)
try:
    codecs.decode(b'x', 'four')
except TypeError as e:
    print('TypeError', e)
codecs.lookup('utf-8')
print(seen)
codecs.unregister(search)
codecs.unregister(search)
try:
    codecs.lookup('rot_x')
except LookupError as e:
    print('LookupError', e)

info = codecs.CodecInfo(str.upper, str.lower)
print(info.name, info.incrementalencoder, info.streamreader, len(info), info[2:])
print(codecs.CodecInfo(1, 2, name='mine').name, isinstance(info, codecs.CodecInfo))

# Error handlers, called directly with fully constructed exceptions.
enc = UnicodeEncodeError('ascii', 'aé€b', 1, 3, 'ordinal not in range(128)')
dec = UnicodeDecodeError('utf-8', b'a\xff\xfeb', 1, 3, 'invalid start byte')
for h in ['ignore', 'replace', 'backslashreplace']:
    print(h, codecs.lookup_error(h)(enc), codecs.lookup_error(h)(dec))
print(codecs.lookup_error('xmlcharrefreplace')(enc), codecs.lookup_error('namereplace')(enc))
for h, arg in [('strict', enc), ('strict', 1), ('ignore', 1), ('replace', ValueError()), ('xmlcharrefreplace', dec)]:
    try:
        codecs.lookup_error(h)(arg)
    except TypeError as e:
        print(h, 'TypeError', e)
    except UnicodeError as e:
        print(h, type(e).__name__, e is arg)
print(codecs.lookup_error('strict') is codecs.strict_errors, codecs.lookup_error('ignore') is codecs.ignore_errors)
def mine(exc):
    return ('<?>', exc.args[3] if len(exc.args) == 5 else exc.end)
codecs.register_error('mine', mine)
print(codecs.lookup_error('mine') is mine, codecs.lookup_error('mine')(enc))
