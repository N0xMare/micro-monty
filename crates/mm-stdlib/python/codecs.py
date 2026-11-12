"""codecs: the codec and error-handler registries (docs/spec.md §12).

Written for micro-monty after CPython's Lib/codecs.py and Python/codecs.c.
The built-in codecs are the ones str.encode and bytes.decode implement
(utf-8, utf-8-sig, ascii, latin-1, utf-16 and utf-32 with their -le/-be forms, and
CPython's aliases). Search functions added with register() are consulted by
lookup(), encode() and decode() only: str.encode and bytes.decode know the
built-in codecs alone, and use the built-in error handlers alone. There are
no incremental or stream codecs.
"""

__all__ = ["register", "unregister", "lookup", "encode", "decode",
           "getencoder", "getdecoder", "register_error", "lookup_error",
           "strict_errors", "ignore_errors", "replace_errors",
           "xmlcharrefreplace_errors", "backslashreplace_errors",
           "namereplace_errors", "CodecInfo",
           "BOM", "BOM_BE", "BOM_LE", "BOM32_BE", "BOM32_LE", "BOM64_BE",
           "BOM64_LE", "BOM_UTF8", "BOM_UTF16", "BOM_UTF16_LE",
           "BOM_UTF16_BE", "BOM_UTF32", "BOM_UTF32_LE", "BOM_UTF32_BE"]

BOM_UTF8 = b'\xef\xbb\xbf'
BOM_LE = BOM_UTF16_LE = b'\xff\xfe'
BOM_BE = BOM_UTF16_BE = b'\xfe\xff'
BOM_UTF32_LE = b'\xff\xfe\x00\x00'
BOM_UTF32_BE = b'\x00\x00\xfe\xff'
# micro-monty is little-endian on every target (sys.byteorder).
BOM = BOM_UTF16 = BOM_UTF16_LE
BOM_UTF32 = BOM_UTF32_LE
BOM32_LE = BOM_UTF16_LE
BOM32_BE = BOM_UTF16_BE
BOM64_LE = BOM_UTF32_LE
BOM64_BE = BOM_UTF32_BE


class CodecInfo(tuple):
    """Codec details when looking up the codec registry."""

    _is_text_encoding = True

    def __new__(cls, encode, decode, streamreader=None, streamwriter=None,
                incrementalencoder=None, incrementaldecoder=None, name=None,
                *, _is_text_encoding=None):
        self = tuple.__new__(cls, (encode, decode, streamreader, streamwriter))
        self.name = name
        self.encode = encode
        self.decode = decode
        self.incrementalencoder = incrementalencoder
        self.incrementaldecoder = incrementaldecoder
        self.streamwriter = streamwriter
        self.streamreader = streamreader
        if _is_text_encoding is not None:
            self._is_text_encoding = _is_text_encoding
        return self

    def __repr__(self):
        return "<%s.%s object for encoding %s at %#x>" % (
            self.__class__.__module__, self.__class__.__qualname__,
            self.name, id(self))


_search = []
_cache = {}

# What 'é' encodes to with errors='replace' tells the built-in codecs apart.
_PROBE = {
    b'\xc3\xa9': 'utf-8',
    b'?': 'ascii',
    b'\xe9': 'iso8859-1',
    b'\xff\xfe\xe9\x00': 'utf-16',
    b'\xe9\x00': 'utf-16-le',
    b'\x00\xe9': 'utf-16-be',
    b'\xff\xfe\x00\x00\xe9\x00\x00\x00': 'utf-32',
    b'\xe9\x00\x00\x00': 'utf-32-le',
    b'\x00\x00\x00\xe9': 'utf-32-be',
    b'\xef\xbb\xbf\xc3\xa9': 'utf-8-sig',
}


def _normalize(encoding):
    # Lower case, each run of characters other than alphanumerics and '.'
    # as one '_', none at either end (encodings.normalize_encoding).
    chars = []
    punct = False
    for c in encoding:
        if c.isalnum() or c == '.':
            if punct and chars:
                chars.append('_')
            chars.append(c.lower())
            punct = False
        else:
            punct = True
    return ''.join(chars)


def _builtin(name):
    def encode(input, errors='strict'):
        if not isinstance(input, str):
            raise TypeError("%s_encode() argument 1 must be str, not %s"
                            % (fn, type(input).__name__))
        return (input.encode(name, errors), len(input))

    def decode(input, errors='strict'):
        if isinstance(input, str) or not hasattr(input, '__len__'):
            raise TypeError("a bytes-like object is required, not '%s'"
                            % type(input).__name__)
        return (bytes(input).decode(name, errors), len(input))

    fn = 'latin_1' if name == 'iso8859-1' else name.replace('-', '_')
    encode.__name__ = encode.__qualname__ = fn + '_encode'
    decode.__name__ = decode.__qualname__ = fn + '_decode'
    return CodecInfo(encode, decode, name=name)


def register(search_function):
    """Register a codec search function."""
    if not callable(search_function):
        raise TypeError("argument must be callable")
    _search.append(search_function)


def unregister(search_function):
    """Unregister a codec search function and clear the registry's cache."""
    if search_function in _search:
        _search.remove(search_function)
        _cache.clear()


def lookup(encoding):
    """Look up a codec tuple in the codec registry and return a CodecInfo."""
    if not isinstance(encoding, str):
        raise TypeError("lookup() argument must be str, not %s"
                        % type(encoding).__name__)
    key = _normalize(encoding)
    found = _cache.get(key)
    if found is not None:
        return found
    try:
        found = _PROBE['\xe9'.encode(key, 'replace')]
    except LookupError:
        pass
    if found is not None:
        found = _builtin(found)
    else:
        for search in _search:
            found = search(key)
            if found is None:
                continue
            if not isinstance(found, tuple) or len(found) != 4:
                raise TypeError("codec search functions must return 4-tuples")
            break
        else:
            raise LookupError("unknown encoding: %s" % encoding)
    _cache[key] = found
    return found


def encode(obj, encoding='utf-8', errors='strict'):
    """Encode obj using the codec registered for encoding."""
    result = lookup(encoding)[0](obj, errors)
    if not isinstance(result, tuple) or len(result) != 2:
        raise TypeError("encoder must return a tuple (object, integer)")
    return result[0]


def decode(obj, encoding='utf-8', errors='strict'):
    """Decode obj using the codec registered for encoding."""
    result = lookup(encoding)[1](obj, errors)
    if not isinstance(result, tuple) or len(result) != 2:
        raise TypeError("decoder must return a tuple (object,integer)")
    return result[0]


def getencoder(encoding):
    """Look up the codec for the given encoding and return its encoder."""
    return lookup(encoding)[0]


def getdecoder(encoding):
    """Look up the codec for the given encoding and return its decoder."""
    return lookup(encoding)[1]


def _span(exc, kinds):
    # (object, start, end) of a UnicodeError. micro-monty's own codec errors
    # carry only their message, so only one constructed with all five
    # arguments (encoding, object, start, end, reason) can be handled.
    if not isinstance(exc, kinds):
        raise TypeError("don't know how to handle %s in error callback"
                        % type(exc).__name__)
    try:
        return exc.object, exc.start, exc.end
    except AttributeError:
        pass
    if len(exc.args) == 5:
        return exc.args[1], exc.args[2], exc.args[3]
    raise TypeError("micro-monty: this %s has no object, start and end"
                    % type(exc).__name__)


def strict_errors(exc):
    """Raise the exception: the 'strict' error handler."""
    if isinstance(exc, BaseException):
        raise exc
    raise TypeError("codec must pass exception instance")


def ignore_errors(exc):
    """Skip the malformed data: the 'ignore' error handler."""
    obj, start, end = _span(exc, (UnicodeEncodeError, UnicodeDecodeError))
    return ('', end)


def replace_errors(exc):
    """Replace the malformed data with '?' or U+FFFD: 'replace'."""
    if isinstance(exc, UnicodeEncodeError):
        obj, start, end = _span(exc, UnicodeEncodeError)
        return ('?' * (end - start), end)
    obj, start, end = _span(exc, UnicodeDecodeError)
    return ('\ufffd', end)


def backslashreplace_errors(exc):
    """Replace with backslashed escape sequences: 'backslashreplace'."""
    if isinstance(exc, UnicodeEncodeError):
        obj, start, end = _span(exc, UnicodeEncodeError)
        return (obj[start:end].encode('ascii', 'backslashreplace').decode('ascii'), end)
    obj, start, end = _span(exc, UnicodeDecodeError)
    return (''.join(['\\x%02x' % b for b in obj[start:end]]), end)


def xmlcharrefreplace_errors(exc):
    """Replace with XML/HTML character references: 'xmlcharrefreplace'."""
    obj, start, end = _span(exc, UnicodeEncodeError)
    return (''.join(['&#%d;' % ord(c) for c in obj[start:end]]), end)


def namereplace_errors(exc):
    """Replace with \\N{...} escape sequences: 'namereplace'."""
    obj, start, end = _span(exc, UnicodeEncodeError)
    return (obj[start:end].encode('ascii', 'namereplace').decode('ascii'), end)


_errors = {
    'strict': strict_errors,
    'ignore': ignore_errors,
    'replace': replace_errors,
    'backslashreplace': backslashreplace_errors,
    'xmlcharrefreplace': xmlcharrefreplace_errors,
    'namereplace': namereplace_errors,
}


def register_error(name, handler):
    """Register handler under name for codecs' error handling."""
    if not isinstance(name, str):
        raise TypeError("register_error() argument 1 must be str, not %s"
                        % type(name).__name__)
    if not callable(handler):
        raise TypeError("handler must be callable")
    _errors[name] = handler


def lookup_error(name):
    """Return the error handler registered under name."""
    if not isinstance(name, str):
        raise TypeError("lookup_error() argument must be str, not %s"
                        % type(name).__name__)
    try:
        return _errors[name]
    except KeyError:
        raise LookupError("unknown error handler name '%s'" % name) from None
