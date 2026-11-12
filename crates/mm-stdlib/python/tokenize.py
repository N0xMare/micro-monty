# tokenize: CPython 3.14's generate_tokens/tokenize API (Lib/tokenize.py,
# PSF license, see THIRD_PARTY.md) over micro-monty's own lexer.
# `_tokens(source)` (native, tokmod.rs) returns the token stream and the
# error that ended it. Left out: untokenize, open, the regular expressions
# and the command line.

import token as _token
from token import *
from collections import namedtuple

__all__ = _token.__all__ + ['tokenize', 'generate_tokens', 'detect_encoding',
                            'TokenInfo', 'TokenError']


class TokenInfo(namedtuple('TokenInfo', 'type string start end line')):
    def __repr__(self):
        annotated_type = '%d (%s)' % (self.type, tok_name[self.type])
        return ('TokenInfo(type=%s, string=%r, start=%r, end=%r, line=%r)' %
                self._replace(type=annotated_type))

    @property
    def exact_type(self):
        if self.type == OP and self.string in EXACT_TOKEN_TYPES:
            return EXACT_TOKEN_TYPES[self.string]
        return self.type


class TokenError(Exception):
    pass


def _read(readline):
    lines = []
    while True:
        try:
            line = readline()
        except StopIteration:
            break
        if not line:
            break
        lines.append(line)
    return lines


def _generate(lines):
    toks, err = _tokens(''.join(lines))
    for t, string, start, end in toks:
        yield TokenInfo(t, string, start, end, ''.join(lines[start[0] - 1:end[0]]))
    if err is not None:
        kind, msg, (lineno, offset) = err
        if kind != 'SyntaxError':
            text = lines[lineno - 1].rstrip('\r\n') if lineno <= len(lines) else ''
            exc = IndentationError if kind == 'IndentationError' else TabError
            raise exc(msg, ('<string>', lineno, len(text) + 1, text, None, None))
        if msg.endswith('was never closed'):
            raise TokenError('unexpected EOF in multi-line statement', (len(lines), 0))
        if msg == 'unexpected EOF while parsing':
            msg = 'unexpected EOF in multi-line statement'
        if 'unterminated triple-quoted string literal' in msg:
            msg = 'EOF in multi-line string'
        raise TokenError(msg, (lineno, offset))


def generate_tokens(readline):
    """Tokenize a source reading Python code as unicode strings."""
    yield from _generate(_read(readline))


BOM_UTF8 = b'\xef\xbb\xbf'


def _get_normal_name(orig_enc):
    enc = orig_enc[:12].lower().replace('_', '-')
    if enc == 'utf-8' or enc.startswith('utf-8-'):
        return 'utf-8'
    if enc in ('latin-1', 'iso-8859-1', 'iso-latin-1') or \
       enc.startswith(('latin-1-', 'iso-8859-1-', 'iso-latin-1-')):
        return 'iso-8859-1'
    return orig_enc


def _cookie(line):
    # CPython's cookie_re: ^[ \t\f]*#.*?coding[:=][ \t]*([-\w.]+)
    text = line.lstrip(' \t\f')
    if not text.startswith('#'):
        return None
    i = text.find('coding')
    while i >= 0:
        rest = text[i + 6:]
        if rest[:1] in (':', '='):
            rest = rest[1:].lstrip(' \t')
            n = 0
            while n < len(rest) and (rest[n].isalnum() or rest[n] in '-_.'):
                n += 1
            if n:
                return rest[:n]
        i = text.find('coding', i + 1)
    return None


def detect_encoding(readline):
    """The encoding of a source read as bytes, and the lines read to find it."""
    lines = []
    bom = False
    default = 'utf-8'
    for n in range(2):
        try:
            line = readline()
        except StopIteration:
            line = b''
        if n == 0 and line.startswith(BOM_UTF8):
            bom = True
            line = line[3:]
            default = 'utf-8-sig'
        if not line:
            break
        lines.append(line)
        try:
            text = line.decode('utf-8')
        except UnicodeDecodeError:
            raise SyntaxError('invalid or missing encoding declaration') from None
        enc = _cookie(text)
        if enc is not None:
            enc = _get_normal_name(enc)
            if bom:
                if enc != 'utf-8':
                    raise SyntaxError('encoding problem: utf-8')
                enc += '-sig'
            return enc, lines
        if text.strip() and not text.lstrip(' \t\f').startswith('#'):
            break
    return default, lines


def tokenize(readline):
    """Tokenize a source read as bytes: an ENCODING token, then its tokens."""
    encoding, first = detect_encoding(readline)
    raw = first + _read(readline)
    yield TokenInfo(ENCODING, encoding, (0, 0), (0, 0), '')
    codec = 'utf-8' if encoding == 'utf-8-sig' else encoding
    yield from _generate([line.decode(codec) for line in raw])
