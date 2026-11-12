"""A collection of string constants (after CPython 3.14's Lib/string, PSF
license, see THIRD_PARTY.md). `Template` scans for `$` substitutions by
hand rather than with `re`, for the default delimiter and identifier
pattern; `Formatter` is not provided."""

__all__ = ["ascii_letters", "ascii_lowercase", "ascii_uppercase", "capwords",
           "digits", "hexdigits", "octdigits", "printable", "punctuation",
           "whitespace", "Template"]

whitespace = ' \t\n\r\v\f'
ascii_lowercase = 'abcdefghijklmnopqrstuvwxyz'
ascii_uppercase = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'
ascii_letters = ascii_lowercase + ascii_uppercase
digits = '0123456789'
hexdigits = digits + 'abcdef' + 'ABCDEF'
octdigits = '01234567'
punctuation = r"""!"#$%&'()*+,-./:;<=>?@[\]^_`{|}~"""
printable = digits + ascii_letters + punctuation + whitespace


def capwords(s, sep=None):
    """capwords(s [,sep]) -> string"""
    return (sep or ' ').join(map(str.capitalize, s.split(sep)))


_sentinel_dict = {}
_ID_START = '_' + ascii_letters
_ID_CHARS = _ID_START + digits


class Template:
    """A string class for supporting $-substitutions."""

    delimiter = '$'

    def __init__(self, template):
        self.template = template

    def _scan(self):
        # Yields (literal, kind, name, start): kind is 'escaped', 'named',
        # 'braced', 'invalid' or None at the end.
        t = self.template
        d = self.delimiter
        i = 0
        n = len(t)
        lit_start = 0
        while True:
            j = t.find(d, i)
            if j < 0:
                yield t[lit_start:], None, None, n
                return
            k = j + len(d)
            if t.startswith(d, k):
                yield t[lit_start:j], 'escaped', None, j
                i = lit_start = k + len(d)
                continue
            if k < n and t[k] in _ID_START:
                e = k + 1
                while e < n and t[e] in _ID_CHARS:
                    e += 1
                yield t[lit_start:j], 'named', t[k:e], j
                i = lit_start = e
                continue
            if k < n and t[k] == '{':
                e = k + 1
                if e < n and t[e] in _ID_START:
                    e += 1
                    while e < n and t[e] in _ID_CHARS:
                        e += 1
                    if e < n and t[e] == '}':
                        yield t[lit_start:j], 'braced', t[k + 1:e], j
                        i = lit_start = e + 1
                        continue
            yield t[lit_start:j], 'invalid', None, k
            i = lit_start = k

    def _invalid(self, i):
        lines = self.template[:i].splitlines(keepends=True)
        if not lines:
            colno = 1
            lineno = 1
        else:
            colno = i - len(''.join(lines[:-1]))
            lineno = len(lines)
        raise ValueError(f'Invalid placeholder in string: line {lineno}, col {colno}')

    def substitute(self, mapping=_sentinel_dict, /, **kws):
        if mapping is _sentinel_dict:
            mapping = kws
        elif kws:
            mapping = _ChainMap(kws, mapping)
        out = []
        for lit, kind, name, start in self._scan():
            out.append(lit)
            if kind == 'escaped':
                out.append(self.delimiter)
            elif kind == 'named' or kind == 'braced':
                out.append(str(mapping[name]))
            elif kind == 'invalid':
                self._invalid(start)
        return ''.join(out)

    def safe_substitute(self, mapping=_sentinel_dict, /, **kws):
        if mapping is _sentinel_dict:
            mapping = kws
        elif kws:
            mapping = _ChainMap(kws, mapping)
        out = []
        t = self.template
        for lit, kind, name, start in self._scan():
            out.append(lit)
            if kind == 'escaped':
                out.append(self.delimiter)
            elif kind == 'named' or kind == 'braced':
                try:
                    out.append(str(mapping[name]))
                except KeyError:
                    if kind == 'named':
                        out.append(self.delimiter + name)
                    else:
                        out.append(self.delimiter + '{' + name + '}')
            elif kind == 'invalid':
                out.append(self.delimiter)
        return ''.join(out)

    def is_valid(self):
        for lit, kind, name, start in self._scan():
            if kind == 'invalid':
                return False
        return True

    def get_identifiers(self):
        ids = []
        for lit, kind, name, start in self._scan():
            if (kind == 'named' or kind == 'braced') and name not in ids:
                ids.append(name)
        return ids


class _ChainMap:
    def __init__(self, *maps):
        self.maps = maps

    def __getitem__(self, key):
        for m in self.maps:
            try:
                return m[key]
            except KeyError:
                pass
        raise KeyError(key)
