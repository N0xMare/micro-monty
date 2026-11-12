"""Base16, Base32, Base64 (RFC 4648), Base85 and Ascii85 data encodings"""

# Adapted from CPython 3.14's Lib/base64.py: no struct, memoryview or
# bytes.translate in micro-monty, so those steps are spelled out (a
# bytearray is taken as bytes); messages unchanged (docs/spec.md §12).

import binascii


__all__ = [
    'encode', 'decode', 'encodebytes', 'decodebytes',
    'b64encode', 'b64decode', 'b32encode', 'b32decode',
    'b32hexencode', 'b32hexdecode', 'b16encode', 'b16decode',
    'b85encode', 'b85decode', 'a85encode', 'a85decode', 'z85encode', 'z85decode',
    'standard_b64encode', 'standard_b64decode',
    'urlsafe_b64encode', 'urlsafe_b64decode',
    ]


bytes_types = (bytes, bytearray)


def _tobytes(s):
    # `memoryview(s).tobytes()`
    if isinstance(s, bytes):
        return s
    if type(s) is bytearray:
        return bytes(s)
    raise TypeError(f"memoryview: a bytes-like object is required, not '{type(s).__name__}'")


def _maketrans(frm, to):
    for x in (frm, to):
        if not isinstance(x, bytes_types):
            raise TypeError(f"a bytes-like object is required, not '{type(x).__name__}'")
    table = list(range(256))
    for a, b in zip(frm, to):
        table[a] = b
    return table


def _translate(s, table):
    return bytes([table[c] for c in s])


def _bytes_from_decode_data(s):
    if isinstance(s, str):
        try:
            return s.encode('ascii')
        except UnicodeEncodeError:
            raise ValueError('string argument should contain only ASCII characters')
    if isinstance(s, bytes_types):
        return s
    raise TypeError("argument should be a bytes-like object or ASCII "
                    f"string, not {s.__class__.__name__!r}") from None


# Base64 encoding/decoding uses binascii

def b64encode(s, altchars=None):
    """Encode the bytes-like object s using Base64 and return a bytes object.

    Optional altchars should be a byte string of length 2 which specifies an
    alternative alphabet for the '+' and '/' characters.  This allows an
    application to e.g. generate url or filesystem safe Base64 strings.
    """
    encoded = binascii.b2a_base64(s, newline=False)
    if altchars is not None:
        assert len(altchars) == 2, repr(altchars)
        return _translate(encoded, _maketrans(b'+/', altchars))
    return encoded


def b64decode(s, altchars=None, validate=False):
    """Decode the Base64 encoded bytes-like object or ASCII string s.

    Optional altchars must be a bytes-like object or ASCII string of length 2
    which specifies the alternative alphabet used instead of the '+' and '/'
    characters.

    The result is returned as a bytes object.  A binascii.Error is raised if
    s is incorrectly padded.

    If validate is False (the default), characters that are neither in the
    normal base-64 alphabet nor the alternative alphabet are discarded prior
    to the padding check.  If validate is True, these non-alphabet characters
    in the input result in a binascii.Error.
    """
    s = _bytes_from_decode_data(s)
    if altchars is not None:
        altchars = _bytes_from_decode_data(altchars)
        assert len(altchars) == 2, repr(altchars)
        s = _translate(s, _maketrans(altchars, b'+/'))
    return binascii.a2b_base64(s, strict_mode=validate)


def standard_b64encode(s):
    """Encode bytes-like object s using the standard Base64 alphabet.

    The result is returned as a bytes object.
    """
    return b64encode(s)


def standard_b64decode(s):
    """Decode bytes encoded with the standard Base64 alphabet.

    Argument s is a bytes-like object or ASCII string to decode.  The result
    is returned as a bytes object.  A binascii.Error is raised if the input
    is incorrectly padded.  Characters that are not in the standard alphabet
    are discarded prior to the padding check.
    """
    return b64decode(s)


def urlsafe_b64encode(s):
    """Encode bytes using the URL- and filesystem-safe Base64 alphabet.

    Argument s is a bytes-like object to encode.  The result is returned as a
    bytes object.  The alphabet uses '-' instead of '+' and '_' instead of
    '/'.
    """
    return b64encode(s).replace(b'+', b'-').replace(b'/', b'_')


def urlsafe_b64decode(s):
    """Decode bytes using the URL- and filesystem-safe Base64 alphabet.

    Argument s is a bytes-like object or ASCII string to decode.  The result
    is returned as a bytes object.  A binascii.Error is raised if the input
    is incorrectly padded.  Characters that are not in the URL-safe base-64
    alphabet, and are not a plus '+' or slash '/', are discarded prior to the
    padding check.

    The alphabet uses '-' instead of '+' and '_' instead of '/'.
    """
    s = _bytes_from_decode_data(s)
    s = _translate(s, _maketrans(b'-_', b'+/'))
    return b64decode(s)


# Base32 encoding/decoding must be done in Python
_b32alphabet = b'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
_b32hexalphabet = b'0123456789ABCDEFGHIJKLMNOPQRSTUV'
_b32rev = {}


def _b32encode(alphabet, s):
    if not isinstance(s, bytes_types):
        s = _tobytes(s)
    leftover = len(s) % 5
    # Pad the last quantum with zero bits if necessary
    if leftover:
        s = s + b'\0' * (5 - leftover)  # Don't use += !
    encoded = []
    for i in range(0, len(s), 5):
        c = int.from_bytes(s[i: i + 5])              # big endian
        for shift in (35, 30, 25, 20, 15, 10, 5, 0):
            encoded.append(alphabet[(c >> shift) & 31])
    # Adjust for any leftover partial quanta
    if leftover:
        n = (0, 6, 4, 3, 1)[leftover]
        encoded[-n:] = b'=' * n
    return bytes(encoded)


def _b32decode(alphabet, s, casefold=False, map01=None):
    if alphabet not in _b32rev:
        _b32rev[alphabet] = {v: k for k, v in enumerate(alphabet)}
    s = _bytes_from_decode_data(s)
    if len(s) % 8:
        raise binascii.Error('Incorrect padding')
    # Handle section 2.4 zero and one mapping.  The flag map01 will be either
    # False, or the character to map the digit 1 (one) to.  It should be
    # either L (el) or I (eye).
    if map01 is not None:
        map01 = _bytes_from_decode_data(map01)
        assert len(map01) == 1, repr(map01)
        s = _translate(s, _maketrans(b'01', b'O' + map01))
    if casefold:
        s = s.upper()
    # Strip off pad characters from the right.  We need to count the pad
    # characters because this will tell us how many null bytes to remove from
    # the end of the decoded string.
    l = len(s)
    s = s.rstrip(b'=')
    padchars = l - len(s)
    # Now decode the full quanta
    decoded = b''
    b32rev = _b32rev[alphabet]
    acc = 0
    parts = []
    for i in range(0, len(s), 8):
        quanta = s[i: i + 8]
        acc = 0
        try:
            for c in quanta:
                acc = (acc << 5) + b32rev[c]
        except KeyError:
            raise binascii.Error('Non-base32 digit found') from None
        parts.append(acc.to_bytes(5))  # big endian
    decoded = b''.join(parts)
    # Process the last, partial quanta
    if l % 8 or padchars not in {0, 1, 3, 4, 6}:
        raise binascii.Error('Incorrect padding')
    if padchars and decoded:
        acc <<= 5 * padchars
        last = acc.to_bytes(5)  # big endian
        leftover = (43 - 5 * padchars) // 8  # 1: 4, 3: 3, 4: 2, 6: 1
        decoded = decoded[:-5] + last[:leftover]
    return decoded


def b32encode(s):
    """Encode the bytes-like objects using base32 and return a bytes object."""
    return _b32encode(_b32alphabet, s)


def b32decode(s, casefold=False, map01=None):
    """Decode the base32 encoded bytes-like object or ASCII string s."""
    return _b32decode(_b32alphabet, s, casefold, map01)


def b32hexencode(s):
    """Encode the bytes-like objects using base32hex and return a bytes object."""
    return _b32encode(_b32hexalphabet, s)


def b32hexdecode(s, casefold=False):
    """Decode the base32hex encoded bytes-like object or ASCII string s."""
    # base32hex does not have the 01 mapping
    return _b32decode(_b32hexalphabet, s, casefold)


# RFC 4648, Base 16 Alphabet specifies uppercase, but hexlify() returns
# lowercase.  The RFC also recommends against accepting input case
# insensitively.
def b16encode(s):
    """Encode the bytes-like object s using Base16 and return a bytes object.
    """
    return binascii.hexlify(s).upper()


def b16decode(s, casefold=False):
    """Decode the Base16 encoded bytes-like object or ASCII string s.

    Optional casefold is a flag specifying whether a lowercase alphabet is
    acceptable as input.  For security purposes, the default is False.

    The result is returned as a bytes object.  A binascii.Error is raised if
    s is incorrectly padded or if there are non-alphabet characters present
    in the input.
    """
    s = _bytes_from_decode_data(s)
    if casefold:
        s = s.upper()
    for c in s:
        if c not in b'0123456789ABCDEF':
            raise binascii.Error('Non-base16 digit found')
    return binascii.unhexlify(s)

#
# Ascii85 encoding/decoding
#

_A85START = b"<~"
_A85END = b"~>"


def _85encode(b, chars, pad=False, foldnuls=False, foldspaces=False):
    # Helper function for a85encode and b85encode
    if not isinstance(b, bytes_types):
        b = _tobytes(b)

    padding = (-len(b)) % 4
    if padding:
        b = b + b'\0' * padding
    chunks = []
    for i in range(0, len(b), 4):
        word = int.from_bytes(b[i:i + 4])
        if foldnuls and not word:
            chunks.append(b'z')
        elif foldspaces and word == 0x20202020:
            chunks.append(b'y')
        else:
            chunks.append(bytes([chars[word // 52200625], chars[word // 614125 % 85],
                                 chars[word // 7225 % 85], chars[word // 85 % 85],
                                 chars[word % 85]]))

    if padding and not pad:
        if chunks[-1] == b'z':
            chunks[-1] = bytes([chars[0]]) * 5
        chunks[-1] = chunks[-1][:-padding]

    return b''.join(chunks)


_a85chars = bytes(range(33, 118))


def a85encode(b, *, foldspaces=False, wrapcol=0, pad=False, adobe=False):
    """Encode bytes-like object b using Ascii85 and return a bytes object.

    foldspaces is an optional flag that uses the special short sequence 'y'
    instead of 4 consecutive spaces (ASCII 0x20) as supported by 'btoa'. This
    feature is not supported by the standard encoding used in PDF.

    If wrapcol is non-zero, insert a newline (b'\\n') character after at most
    every wrapcol characters.

    pad controls whether zero-padding applied to the end of the input
    is fully retained in the output encoding, as done by btoa,
    producing an exact multiple of 5 bytes of output.

    adobe controls whether the encoded byte sequence is framed with <~
    and ~>, as in a PostScript base-85 string literal.  Note that
    while ASCII85Decode streams in PDF documents must be terminated
    with ~>, they must not use a leading <~.

    """
    result = _85encode(b, _a85chars, pad, True, foldspaces)

    if adobe:
        result = _A85START + result
    if wrapcol:
        wrapcol = max(2 if adobe else 1, wrapcol)
        chunks = [result[i: i + wrapcol]
                  for i in range(0, len(result), wrapcol)]
        if adobe:
            if len(chunks[-1]) + 2 > wrapcol:
                chunks.append(b'')
        result = b'\n'.join(chunks)
    if adobe:
        result += _A85END

    return result


def a85decode(b, *, foldspaces=False, adobe=False, ignorechars=b' \t\n\r\v'):
    """Decode the Ascii85 encoded bytes-like object or ASCII string b.

    foldspaces is a flag that specifies whether the 'y' short sequence
    should be accepted as shorthand for 4 consecutive spaces (ASCII
    0x20).  This feature is not supported by the standard Ascii85
    encoding used in PDF and PostScript.

    adobe controls whether the <~ and ~> markers are present. While
    the leading <~ is not required, the input must end with ~>, or a
    ValueError is raised.

    ignorechars should be a byte string containing characters to ignore from the
    input. This should only contain whitespace characters, and by default
    contains all whitespace characters in ASCII.

    The result is returned as a bytes object.
    """
    b = _bytes_from_decode_data(b)
    if adobe:
        if not b.endswith(_A85END):
            raise ValueError(
                "Ascii85 encoded byte sequences must end "
                f"with {_A85END!r}"
                )
        if b.startswith(_A85START):
            b = b[2:-2]  # Strip off start/end markers
        else:
            b = b[:-2]
    #
    # We have to go through this stepwise, so as to ignore spaces and handle
    # special short sequences
    #
    decoded = []
    curr = []
    for x in b + b'u' * 4:
        if 33 <= x <= 117:
            curr.append(x)
            if len(curr) == 5:
                acc = 0
                for x in curr:
                    acc = 85 * acc + (x - 33)
                if acc > 0xFFFFFFFF:
                    raise ValueError('Ascii85 overflow') from None
                decoded.append(acc.to_bytes(4))
                curr.clear()
        elif x == 122:
            if curr:
                raise ValueError('z inside Ascii85 5-tuple')
            decoded.append(b'\0\0\0\0')
        elif foldspaces and x == 121:
            if curr:
                raise ValueError('y inside Ascii85 5-tuple')
            decoded.append(b'\x20\x20\x20\x20')
        elif x in ignorechars:
            # Skip whitespace
            continue
        else:
            raise ValueError(f'Non-Ascii85 digit found: {chr(x)}')

    result = b''.join(decoded)
    padding = 4 - len(curr)
    if padding:
        # Throw away the extra padding
        result = result[:-padding]
    return result

# The following code is originally taken (with permission) from Mercurial

_b85alphabet = (b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
                b"abcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~")
_b85dec = None


def b85encode(b, pad=False):
    """Encode bytes-like object b in base85 format and return a bytes object.

    The input is padded with b'\0' so its length is a multiple of 4
    bytes before encoding.  If pad is true, all the resulting
    characters are retained in the output, which will always be a
    multiple of 5 bytes.
    """
    return _85encode(b, _b85alphabet, pad)


def b85decode(b):
    """Decode the base85-encoded bytes-like object or ASCII string b

    The result is returned as a bytes object.
    """
    global _b85dec
    if _b85dec is None:
        b85dec_tmp = [None] * 256
        for i, c in enumerate(_b85alphabet):
            b85dec_tmp[c] = i
        _b85dec = b85dec_tmp

    b = _bytes_from_decode_data(b)
    padding = (-len(b)) % 5
    b = b + b'~' * padding
    out = []
    for i in range(0, len(b), 5):
        chunk = b[i:i + 5]
        acc = 0
        for j, c in enumerate(chunk):
            d = _b85dec[c]
            if d is None:
                raise ValueError(f'bad base85 character at position {i + j}') from None
            acc = acc * 85 + d
        if acc > 0xFFFFFFFF:
            raise ValueError(f'base85 overflow in hunk starting at byte {i}') from None
        out.append(acc.to_bytes(4))

    result = b''.join(out)
    if padding:
        result = result[:-padding]
    return result


_z85alphabet = (b'0123456789abcdefghijklmnopqrstuvwxyz'
                b'ABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?&<>()[]{}@%$#')
# Translating b85 valid but z85 invalid chars to b'\x00' is required
# to prevent them from being decoded as b85 valid chars.
_z85_b85_decode_diff = b';_`|~'


def z85encode(s):
    """Encode bytes-like object b in z85 format and return a bytes object."""
    return _translate(b85encode(s), _maketrans(_b85alphabet, _z85alphabet))


def z85decode(s):
    """Decode the z85-encoded bytes-like object or ASCII string b

    The result is returned as a bytes object.
    """
    s = _bytes_from_decode_data(s)
    s = _translate(s, _maketrans(_z85alphabet + _z85_b85_decode_diff,
                                 _b85alphabet + b'\x00' * len(_z85_b85_decode_diff)))
    try:
        return b85decode(s)
    except ValueError as e:
        raise ValueError(e.args[0].replace('base85', 'z85')) from None

# Legacy interface.

MAXLINESIZE = 76 # Excluding the CRLF
MAXBINSIZE = (MAXLINESIZE//4)*3


def encode(input, output):
    """Encode a file; input and output are binary files."""
    while s := input.read(MAXBINSIZE):
        while len(s) < MAXBINSIZE and (ns := input.read(MAXBINSIZE-len(s))):
            s += ns
        line = binascii.b2a_base64(s)
        output.write(line)


def decode(input, output):
    """Decode a file; input and output are binary files."""
    while line := input.readline():
        s = binascii.a2b_base64(line)
        output.write(s)


def _input_type_check(s):
    if not isinstance(s, bytes_types):
        raise TypeError(f'expected bytes-like object, not {s.__class__.__name__}')


def encodebytes(s):
    """Encode a bytestring into a bytes object containing multiple lines
    of base-64 data."""
    _input_type_check(s)
    pieces = []
    for i in range(0, len(s), MAXBINSIZE):
        chunk = s[i : i + MAXBINSIZE]
        pieces.append(binascii.b2a_base64(chunk))
    return b"".join(pieces)


def decodebytes(s):
    """Decode a bytestring of base-64 data into a bytes object."""
    _input_type_check(s)
    return binascii.a2b_base64(s)
