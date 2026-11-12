# binascii: base64, hex and the CRCs are native (mm-engine's binascii.rs);
# uuencode and quoted-printable follow CPython's Modules/binascii.c here,
# with Argument Clinic's messages (docs/spec.md §12).

_MISSING = object()


class Error(ValueError):
    pass


Error.__module__ = 'binascii'


class Incomplete(Exception):
    pass


Incomplete.__module__ = 'binascii'


def _clinic(fname, args, kwargs, names, maxpos, minpos, posonly=0):
    # Argument Clinic's `_PyArg_UnpackKeywords`, with its messages.
    nargs = len(args)
    nkw = len(kwargs)
    maxargs = len(names)
    if nargs + nkw > maxargs:
        raise TypeError(f"{fname}() takes at most {maxargs} {'keyword ' if nargs == 0 else ''}"
                        f"argument{'' if maxargs == 1 else 's'} ({nargs + nkw} given)")
    if nargs > maxpos:
        if maxpos == 0:
            raise TypeError(f'{fname}() takes no positional arguments')
        raise TypeError(f"{fname}() takes {'at most' if minpos < maxpos else 'exactly'} {maxpos} "
                        f"positional argument{'' if maxpos == 1 else 's'} ({nargs} given)")
    if nargs < posonly:
        raise TypeError(f"{fname}() takes {'at least' if posonly < maxpos else 'exactly'} "
                        f"{posonly} positional argument{'' if posonly == 1 else 's'} "
                        f'({nargs} given)')
    out = list(args) + [_MISSING] * (maxargs - nargs)
    left = nkw
    for i in range(max(nargs, posonly), maxargs):
        if left:
            if names[i] in kwargs:
                out[i] = kwargs[names[i]]
                left -= 1
        elif i >= minpos:
            break
        if out[i] is _MISSING and i < minpos:
            raise TypeError(f"{fname}() missing required argument '{names[i]}' (pos {i + 1})")
    if left:
        for i in range(posonly, nargs):
            if names[i] in kwargs:
                raise TypeError(f"argument for {fname}() given by name ('{names[i]}') and "
                                f'position ({i + 1})')
        for k in kwargs:
            if k not in names[posonly:]:
                raise TypeError(f"{fname}() got an unexpected keyword argument '{k}'")
    return out


def _meth_o(fname, args, kwargs):
    if kwargs:
        raise TypeError(f'binascii.{fname}() takes no keyword arguments')
    if len(args) != 1:
        raise TypeError(f'binascii.{fname}() takes exactly one argument ({len(args)} given)')
    return args[0]


def _buffer(data):
    if type(data) is bytearray:
        return bytes(data)
    if not isinstance(data, bytes):
        raise TypeError(f"a bytes-like object is required, not '{type(data).__name__}'")
    return data


def _ascii(data):
    # Argument Clinic's `ascii_buffer` converter.
    if isinstance(data, str):
        for c in data:
            if ord(c) > 127:
                raise ValueError('string argument should contain only ASCII characters')
        return data.encode()
    if type(data) is bytearray:
        return bytes(data)
    if not isinstance(data, bytes):
        raise TypeError('argument should be bytes, buffer or ASCII string, '
                        f"not '{type(data).__name__}'")
    return data


def _uint(x):
    # `unsigned_int(bitwise=True)` / `unsigned_long(bitwise=True)`: modulo 2**32.
    if not isinstance(x, int):
        if not hasattr(type(x), '__index__'):
            raise TypeError(f"'{type(x).__name__}' object cannot be interpreted as an integer")
        x = x.__index__()
    return int(x) & 0xFFFFFFFF


def _check(r):
    if isinstance(r, str):
        raise Error(r)
    return r


def a2b_base64(*args, **kwargs):
    """Decode a line of base64 data."""
    data, strict = _clinic('a2b_base64', args, kwargs, ('data', 'strict_mode'), 1, 1, 1)
    data = _ascii(data)
    return _check(_a2b_base64(data, strict is not _MISSING and bool(strict)))


def b2a_base64(*args, **kwargs):
    """Base64-code line of data."""
    data, newline = _clinic('b2a_base64', args, kwargs, ('data', 'newline'), 1, 1, 1)
    data = _buffer(data)
    return _b2a_base64(data, newline is _MISSING or bool(newline))


def hexlify(*args, **kwargs):
    """Hexadecimal representation of binary data."""
    data, sep, per = _clinic('hexlify', args, kwargs, ('data', 'sep', 'bytes_per_sep'), 3, 1)
    data = _buffer(data)
    per = 1 if per is _MISSING else _ssize(per)
    if sep is _MISSING:
        return _hexlify(data)
    if len(sep) != 1:
        raise ValueError('sep must be length 1.')
    if isinstance(sep, str):
        if ord(sep) > 255:
            raise ValueError('sep must be ASCII.')
        sep = bytes([ord(sep)])
    elif not isinstance(sep, bytes):
        raise TypeError('sep must be str or bytes.')
    if sep[0] > 127:
        # bytes.hex only takes an ASCII separator; splice a placeholder.
        return data.hex('-', per).encode().replace(b'-', sep)
    return data.hex(sep, per).encode()


def b2a_hex(*args, **kwargs):
    """Hexadecimal representation of binary data."""
    return hexlify(*args, **kwargs)


def unhexlify(*args, **kwargs):
    """Binary data of hexadecimal representation."""
    return _check(_unhexlify(_ascii(_meth_o('unhexlify', args, kwargs))))


def a2b_hex(*args, **kwargs):
    """Binary data of hexadecimal representation."""
    return _check(_unhexlify(_ascii(_meth_o('a2b_hex', args, kwargs))))


def _ssize(x):
    if not isinstance(x, int):
        if not hasattr(type(x), '__index__'):
            raise TypeError(f"'{type(x).__name__}' object cannot be interpreted as an integer")
        x = x.__index__()
    return int(x)


def crc32(*args, **kwargs):
    """Compute CRC-32 incrementally."""
    if kwargs:
        raise TypeError('binascii.crc32() takes no keyword arguments')
    if not args:
        raise TypeError('crc32 expected at least 1 argument, got 0')
    if len(args) > 2:
        raise TypeError(f'crc32 expected at most 2 arguments, got {len(args)}')
    data = _buffer(args[0])
    return _crc32(data, _uint(args[1]) if len(args) > 1 else 0)


def crc_hqx(*args, **kwargs):
    """Compute CRC-CCITT incrementally."""
    if kwargs:
        raise TypeError('binascii.crc_hqx() takes no keyword arguments')
    if len(args) != 2:
        raise TypeError(f'crc_hqx expected 2 arguments, got {len(args)}')
    data = _buffer(args[0])
    return _crc_hqx(data, _uint(args[1]))


def a2b_uu(*args, **kwargs):
    """Decode a line of uuencoded data."""
    data = _ascii(_meth_o('a2b_uu', args, kwargs))
    n = len(data)
    if not n:
        raise Error('Missing length byte')
    want = (data[0] - 32) & 0o77
    out = []
    i = 1
    left = 0
    bits = 0
    while want > 0:
        c = data[i] if i < n else 0
        if c == 10 or c == 13 or i >= n:
            c = 0
        else:
            if c < 32 or c > 96:
                raise Error('Illegal char')
            c = (c - 32) & 0o77
        left = (left << 6) | c
        bits += 6
        if bits >= 8:
            bits -= 8
            out.append((left >> bits) & 0xFF)
            left &= (1 << bits) - 1
            want -= 1
        i += 1
    while i < n:
        c = data[i]
        if c != 32 and c != 96 and c != 10 and c != 13:
            raise Error('Trailing garbage')
        i += 1
    return bytes(out)


def b2a_uu(*args, **kwargs):
    """Uuencode line of data."""
    data, backtick = _clinic('b2a_uu', args, kwargs, ('data', 'backtick'), 1, 1, 1)
    data = _buffer(data)
    backtick = backtick is not _MISSING and bool(backtick)
    n = len(data)
    if n > 45:
        raise Error('At most 45 bytes at once')
    out = [96 if backtick and not n else 32 + n]
    for i in range(0, n, 3):
        w = int.from_bytes(data[i:i + 3] + bytes(3 - len(data[i:i + 3])), 'big')
        for shift in (18, 12, 6, 0):
            c = (w >> shift) & 0x3F
            out.append(96 if backtick and not c else c + 32)
    out.append(10)
    return bytes(out)


def a2b_qp(*args, **kwargs):
    """Decode a string of qp-encoded data."""
    data, header = _clinic('a2b_qp', args, kwargs, ('data', 'header'), 2, 1)
    data = _ascii(data)
    header = header is not _MISSING and bool(header)
    hexd = b'0123456789ABCDEFabcdef'
    out = []
    n = len(data)
    i = 0
    while i < n:
        c = data[i]
        if c == 61:
            i += 1
            if i >= n:
                break
            c = data[i]
            if c == 10 or c == 13:
                if c != 10:
                    while i < n and data[i] != 10:
                        i += 1
                if i < n:
                    i += 1
            elif c == 61:
                out.append(61)
                i += 1
            elif i + 1 < n and c in hexd and data[i + 1] in hexd:
                out.append(int(data[i:i + 2], 16))
                i += 2
            else:
                out.append(61)
        elif header and c == 95:
            out.append(32)
            i += 1
        else:
            out.append(c)
            i += 1
    return bytes(out)


def b2a_qp(*args, **kwargs):
    """Encode a string using quoted-printable encoding."""
    data, quotetabs, istext, header = _clinic(
        'b2a_qp', args, kwargs, ('data', 'quotetabs', 'istext', 'header'), 4, 1)
    data = _buffer(data)
    quotetabs = quotetabs is not _MISSING and bool(quotetabs)
    istext = istext is _MISSING or bool(istext)
    header = header is not _MISSING and bool(header)
    p = data.find(b'\n')
    crlf = p > 0 and data[p - 1] == 13
    brk = b'=\r\n' if crlf else b'=\n'
    eol = b'\r\n' if crlf else b'\n'
    n = len(data)
    out = []
    linelen = 0
    i = 0
    while i < n:
        c = data[i]
        nxt = data[i + 1] if i + 1 < n else -1
        if (c > 126 or c == 61 or (header and c == 95)
                or (c == 46 and linelen == 0 and nxt in (-1, 10, 13, 0))
                or (not istext and (c == 13 or c == 10))
                or ((c == 9 or c == 32) and i + 1 == n)
                or (c < 33 and c != 13 and c != 10 and (quotetabs or (c != 9 and c != 32)))):
            if linelen + 3 >= 76:
                out.extend(brk)
                linelen = 0
            out.extend(b'=' + bytes([c]).hex().upper().encode())
            i += 1
            linelen += 3
        elif istext and (c == 10 or (c == 13 and nxt == 10)):
            linelen = 0
            if out and (out[-1] == 32 or out[-1] == 9):
                out[-1:] = b'=' + bytes([out[-1]]).hex().upper().encode()
            out.extend(eol)
            i += 2 if c == 13 else 1
        else:
            if i + 1 != n and nxt != 10 and linelen + 1 >= 76:
                out.extend(brk)
                linelen = 0
            linelen += 1
            out.append(95 if header and c == 32 else c)
            i += 1
    return bytes(out)
