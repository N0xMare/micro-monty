# re: regular expressions (docs/spec.md §12 `re`).
#
# The pattern parser and the replacement-template parser are ported from
# CPython 3.14's Lib/re/_parser.py, and the module surface follows
# Lib/re/__init__.py (Secret Labs' Regular Expression Engine, Copyright (c)
# 1998-2001 by Secret Labs AB; Python Software Foundation License, see
# THIRD_PARTY.md). The code generator is micro-monty's own: it compiles the
# parse tree to the program the native matcher (`_exec`, mm-engine's re.rs)
# runs, in slices, so that every loop iteration below is a safepoint.

_MAXSIZE = 9223372036854775807
MAXREPEAT = 4294967295
MAXGROUPS = 524287
MAXWIDTH = 1 << 64

# Flags (sre's values).
NOFLAG = 0
_TEMPLATE = 1
_IGNORECASE = 2
_LOCALE = 4
_MULTILINE = 8
_DOTALL = 16
_UNICODE = 32
_VERBOSE = 64
_DEBUG = 128
_ASCII = 256

_FLAG_NAMES = [
    (_IGNORECASE, 'IGNORECASE'), (_LOCALE, 'LOCALE'), (_MULTILINE, 'MULTILINE'),
    (_DOTALL, 'DOTALL'), (_UNICODE, 'UNICODE'), (_VERBOSE, 'VERBOSE'),
    (_DEBUG, 'DEBUG'), (_ASCII, 'ASCII'),
]


def _flags_text(flags):
    names = []
    rest = flags
    for bit, name in _FLAG_NAMES:
        if flags & bit:
            names.append('re.' + name)
            rest &= ~bit
    if rest:
        names.append(hex(rest))
    return '|'.join(names)


class RegexFlag(int):
    def __repr__(self):
        v = int(self)
        if v == 0:
            return 're.NOFLAG'
        if v < 0:
            return 're.' + _flags_text(~v).replace('re.', '~re.')
        return _flags_text(v)

    __str__ = __repr__

    def __or__(self, other):
        if isinstance(other, int):
            return RegexFlag(int(self) | int(other))
        return NotImplemented

    __ror__ = __or__

    def __and__(self, other):
        if isinstance(other, int):
            return RegexFlag(int(self) & int(other))
        return NotImplemented

    __rand__ = __and__

    def __xor__(self, other):
        if isinstance(other, int):
            return RegexFlag(int(self) ^ int(other))
        return NotImplemented

    __rxor__ = __xor__

    def __invert__(self):
        return RegexFlag(~int(self) & 0x1FF)

    @property
    def value(self):
        return int(self)

    @property
    def name(self):
        return _flags_text(int(self)).replace('re.', '') or None


A = ASCII = RegexFlag(_ASCII)
I = IGNORECASE = RegexFlag(_IGNORECASE)
L = LOCALE = RegexFlag(_LOCALE)
U = UNICODE = RegexFlag(_UNICODE)
M = MULTILINE = RegexFlag(_MULTILINE)
S = DOTALL = RegexFlag(_DOTALL)
X = VERBOSE = RegexFlag(_VERBOSE)
DEBUG = RegexFlag(_DEBUG)
NOFLAG = RegexFlag(0)


class PatternError(Exception):
    def __init__(self, msg, pattern=None, pos=None):
        self.msg = msg
        self.pattern = pattern
        self.pos = pos
        if pattern is not None and pos is not None:
            msg = f'{msg} at position {pos}'
            if isinstance(pattern, str):
                newline = '\n'
            else:
                newline = b'\n'
            self.lineno = pattern.count(newline, 0, pos) + 1
            self.colno = pos - pattern.rfind(newline, 0, pos)
            if newline in pattern:
                msg = f'{msg} (line {self.lineno}, column {self.colno})'
        else:
            self.lineno = self.colno = None
        super().__init__(msg)


error = PatternError

# ---------------------------------------------------------------- parse tree

FAILURE = 0
ANY = 2
ASSERT = 4
ASSERT_NOT = 5
AT = 6
BRANCH = 7
CATEGORY = 8
GROUPREF = 9
GROUPREF_EXISTS = 10
IN = 11
LITERAL = 12
MAX_REPEAT = 14
MIN_REPEAT = 15
POSSESSIVE_REPEAT = 16
NEGATE = 17
NOT_LITERAL = 18
RANGE = 19
SUBPATTERN = 20
ATOMIC_GROUP = 21

AT_BEGINNING = 0
AT_BEGINNING_STRING = 1
AT_BOUNDARY = 2
AT_NON_BOUNDARY = 3
AT_END = 4
AT_END_STRING = 5

CATEGORY_DIGIT = 0
CATEGORY_NOT_DIGIT = 1
CATEGORY_SPACE = 2
CATEGORY_NOT_SPACE = 3
CATEGORY_WORD = 4
CATEGORY_NOT_WORD = 5

SPECIAL_CHARS = ".\\[{()*+?^$|"
REPEAT_CHARS = "*+?{"
DIGITS = set("0123456789")
OCTDIGITS = set("01234567")
HEXDIGITS = set("0123456789abcdefABCDEF")
ASCIILETTERS = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ")
WHITESPACE = set(" \t\n\r\v\f")

_REPEATCODES = (MIN_REPEAT, MAX_REPEAT, POSSESSIVE_REPEAT)
_UNITCODES = (ANY, RANGE, IN, LITERAL, NOT_LITERAL, CATEGORY)

ESCAPES = {
    "\\a": (LITERAL, 7),
    "\\b": (LITERAL, 8),
    "\\f": (LITERAL, 12),
    "\\n": (LITERAL, 10),
    "\\r": (LITERAL, 13),
    "\\t": (LITERAL, 9),
    "\\v": (LITERAL, 11),
    "\\\\": (LITERAL, 92),
}

CATEGORIES = {
    "\\A": (AT, AT_BEGINNING_STRING),
    "\\b": (AT, AT_BOUNDARY),
    "\\B": (AT, AT_NON_BOUNDARY),
    "\\d": (IN, [(CATEGORY, CATEGORY_DIGIT)]),
    "\\D": (IN, [(CATEGORY, CATEGORY_NOT_DIGIT)]),
    "\\s": (IN, [(CATEGORY, CATEGORY_SPACE)]),
    "\\S": (IN, [(CATEGORY, CATEGORY_NOT_SPACE)]),
    "\\w": (IN, [(CATEGORY, CATEGORY_WORD)]),
    "\\W": (IN, [(CATEGORY, CATEGORY_NOT_WORD)]),
    "\\z": (AT, AT_END_STRING),
    "\\Z": (AT, AT_END_STRING),
}

FLAGS = {
    "i": _IGNORECASE,
    "L": _LOCALE,
    "m": _MULTILINE,
    "s": _DOTALL,
    "x": _VERBOSE,
    "a": _ASCII,
    "u": _UNICODE,
}

TYPE_FLAGS = _ASCII | _LOCALE | _UNICODE
GLOBAL_FLAGS = _DEBUG


def _is_ascii_decimal(s):
    if not s:
        return False
    for c in s:
        if c not in DIGITS:
            return False
    return True


def _is_ascii(s):
    for c in s:
        if ord(c) > 127:
            return False
    return True


class _State:
    def __init__(self):
        self.flags = 0
        self.groupdict = {}
        self.groupwidths = [None]
        self.lookbehindgroups = None
        self.grouprefpos = {}

    @property
    def groups(self):
        return len(self.groupwidths)

    def opengroup(self, name=None):
        gid = self.groups
        self.groupwidths.append(None)
        if self.groups > MAXGROUPS:
            raise error("too many groups")
        if name is not None:
            ogid = self.groupdict.get(name, None)
            if ogid is not None:
                raise error(f"redefinition of group name {name!r} as group {gid}; was group {ogid}")
            self.groupdict[name] = gid
        return gid

    def closegroup(self, gid, p):
        self.groupwidths[gid] = p.getwidth()

    def checkgroup(self, gid):
        return gid < self.groups and self.groupwidths[gid] is not None

    def checklookbehindgroup(self, gid, source):
        if self.lookbehindgroups is not None:
            if not self.checkgroup(gid):
                raise source.error('cannot refer to an open group')
            if gid >= self.lookbehindgroups:
                raise source.error('cannot refer to group defined in the same lookbehind subpattern')


class _SubPattern:
    def __init__(self, state, data=None):
        self.state = state
        if data is None:
            data = []
        self.data = data
        self.width = None

    def __len__(self):
        return len(self.data)

    def __bool__(self):
        return len(self.data) != 0

    def __delitem__(self, index):
        del self.data[index]

    def __getitem__(self, index):
        if isinstance(index, slice):
            return _SubPattern(self.state, self.data[index])
        return self.data[index]

    def __setitem__(self, index, code):
        self.data[index] = code

    def append(self, code):
        self.data.append(code)

    def getwidth(self):
        if self.width is not None:
            return self.width
        lo = hi = 0
        for op, av in self.data:
            if op == BRANCH:
                i = MAXWIDTH
                j = 0
                for av in av[1]:
                    l, h = av.getwidth()
                    i = min(i, l)
                    j = max(j, h)
                lo = lo + i
                hi = hi + j
            elif op == ATOMIC_GROUP:
                i, j = av.getwidth()
                lo = lo + i
                hi = hi + j
            elif op == SUBPATTERN:
                i, j = av[-1].getwidth()
                lo = lo + i
                hi = hi + j
            elif op in _REPEATCODES:
                i, j = av[2].getwidth()
                lo = lo + i * av[0]
                if av[1] == MAXREPEAT and j:
                    hi = MAXWIDTH
                else:
                    hi = hi + j * av[1]
            elif op in _UNITCODES:
                lo = lo + 1
                hi = hi + 1
            elif op == GROUPREF:
                i, j = self.state.groupwidths[av]
                lo = lo + i
                hi = hi + j
            elif op == GROUPREF_EXISTS:
                i, j = av[1].getwidth()
                if av[2] is not None:
                    l, h = av[2].getwidth()
                    i = min(i, l)
                    j = max(j, h)
                else:
                    i = 0
                lo = lo + i
                hi = hi + j
        self.width = min(lo, MAXWIDTH), min(hi, MAXWIDTH)
        return self.width


class _Tokenizer:
    def __init__(self, string):
        self.istext = isinstance(string, str)
        self.string = string
        if not self.istext:
            string = str(string, 'latin1')
        self.decoded_string = string
        self.index = 0
        self.next = None
        self._next()

    def _next(self):
        index = self.index
        if index >= len(self.decoded_string):
            self.next = None
            return
        char = self.decoded_string[index]
        if char == "\\":
            index += 1
            if index >= len(self.decoded_string):
                raise error("bad escape (end of pattern)", self.string, len(self.string) - 1)
            char += self.decoded_string[index]
        self.index = index + 1
        self.next = char

    def match(self, char):
        if char == self.next:
            self._next()
            return True
        return False

    def get(self):
        this = self.next
        self._next()
        return this

    def getwhile(self, n, charset):
        result = ''
        for _ in range(n):
            c = self.next
            if c not in charset:
                break
            result += c
            self._next()
        return result

    def getuntil(self, terminator, name):
        result = ''
        while True:
            c = self.next
            self._next()
            if c is None:
                if not result:
                    raise self.error("missing " + name)
                raise self.error(f"missing {terminator}, unterminated name", len(result))
            if c == terminator:
                if not result:
                    raise self.error("missing " + name, 1)
                break
            result += c
        return result

    @property
    def pos(self):
        return self.index - len(self.next or '')

    def tell(self):
        return self.index - len(self.next or '')

    def seek(self, index):
        self.index = index
        self._next()

    def error(self, msg, offset=0):
        if not self.istext:
            msg = msg.encode('ascii', 'backslashreplace').decode('ascii')
        return error(msg, self.string, self.tell() - offset)

    def checkgroupname(self, name, offset):
        if not (self.istext or _is_ascii(name)):
            raise self.error(f"bad character in group name {ascii(name)}", len(name) + offset)
        if not name.isidentifier():
            raise self.error(f"bad character in group name {name!r}", len(name) + offset)


def _hexval(s):
    return int(s, 16)


def _class_escape(source, escape):
    code = ESCAPES.get(escape)
    if code:
        return code
    code = CATEGORIES.get(escape)
    if code and code[0] == IN:
        return code
    c = escape[1:2]
    if c == "x":
        escape += source.getwhile(2, HEXDIGITS)
        if len(escape) != 4:
            raise source.error("incomplete escape " + escape, len(escape))
        return LITERAL, _hexval(escape[2:])
    elif c == "u" and source.istext:
        escape += source.getwhile(4, HEXDIGITS)
        if len(escape) != 6:
            raise source.error("incomplete escape " + escape, len(escape))
        return LITERAL, _hexval(escape[2:])
    elif c == "U" and source.istext:
        escape += source.getwhile(8, HEXDIGITS)
        if len(escape) != 10:
            raise source.error("incomplete escape " + escape, len(escape))
        c = _hexval(escape[2:])
        if c > 0x10FFFF:
            raise source.error("bad escape " + escape, len(escape))
        return LITERAL, c
    elif c == "N" and source.istext:
        return _named_escape(source)
    elif c in OCTDIGITS:
        escape += source.getwhile(2, OCTDIGITS)
        c = int(escape[1:], 8)
        if c > 0o377:
            raise source.error(f'octal escape value {escape} outside of range 0-0o377', len(escape))
        return LITERAL, c
    elif c in DIGITS:
        raise source.error("bad escape " + escape, len(escape))
    if len(escape) == 2:
        if c in ASCIILETTERS:
            raise source.error('bad escape ' + escape, len(escape))
        return LITERAL, ord(escape[1])
    raise source.error("bad escape " + escape, len(escape))


def _named_escape(source):
    # micro-monty bundles no Unicode name table (docs/spec.md §6).
    if not source.match('{'):
        raise source.error("missing {")
    charname = source.getuntil('}', 'character name')
    raise source.error(f"undefined character name {charname!r}", len(charname) + 4)


def _escape(source, escape, state):
    code = CATEGORIES.get(escape)
    if code:
        return code
    code = ESCAPES.get(escape)
    if code:
        return code
    c = escape[1:2]
    if c == "x":
        escape += source.getwhile(2, HEXDIGITS)
        if len(escape) != 4:
            raise source.error("incomplete escape " + escape, len(escape))
        return LITERAL, _hexval(escape[2:])
    elif c == "u" and source.istext:
        escape += source.getwhile(4, HEXDIGITS)
        if len(escape) != 6:
            raise source.error("incomplete escape " + escape, len(escape))
        return LITERAL, _hexval(escape[2:])
    elif c == "U" and source.istext:
        escape += source.getwhile(8, HEXDIGITS)
        if len(escape) != 10:
            raise source.error("incomplete escape " + escape, len(escape))
        c = _hexval(escape[2:])
        if c > 0x10FFFF:
            raise source.error("bad escape " + escape, len(escape))
        return LITERAL, c
    elif c == "N" and source.istext:
        return _named_escape(source)
    elif c == "0":
        escape += source.getwhile(2, OCTDIGITS)
        return LITERAL, int(escape[1:], 8)
    elif c in DIGITS:
        if source.next in DIGITS:
            escape += source.get()
            if (escape[1] in OCTDIGITS and escape[2] in OCTDIGITS and
                    source.next in OCTDIGITS):
                escape += source.get()
                c = int(escape[1:], 8)
                if c > 0o377:
                    raise source.error(f'octal escape value {escape} outside of range 0-0o377', len(escape))
                return LITERAL, c
        group = int(escape[1:])
        if group < state.groups:
            if not state.checkgroup(group):
                raise source.error("cannot refer to an open group", len(escape))
            state.checklookbehindgroup(group, source)
            return GROUPREF, group
        raise source.error(f"invalid group reference {group}", len(escape) - 1)
    if len(escape) == 2:
        if c in ASCIILETTERS:
            raise source.error("bad escape " + escape, len(escape))
        return LITERAL, ord(escape[1])
    raise source.error("bad escape " + escape, len(escape))


def _warn(message, category):
    # CPython's warnings; a build without the warnings module drops them.
    try:
        import warnings
    except ImportError:
        return
    # The first frame outside re, where CPython's computed stacklevel lands.
    warnings.warn(message, category, stacklevel=2, skip_file_prefixes=("<re>",))


def _uniq(items):
    return list(dict.fromkeys(items))


def _parse_sub(source, state, verbose, nested):
    items = []
    start = source.tell()
    while True:
        items.append(_parse(source, state, verbose, nested + 1, not nested and not items))
        if not source.match("|"):
            break
        if not nested:
            verbose = state.flags & _VERBOSE

    if len(items) == 1:
        return items[0]

    subpattern = _SubPattern(state)

    # Move a prefix all alternatives share out of the branch.
    while True:
        prefix = None
        common = True
        for item in items:
            if not item:
                common = False
                break
            if prefix is None:
                prefix = item[0]
            elif item[0] != prefix:
                common = False
                break
        if not common:
            break
        for item in items:
            del item[0]
        subpattern.append(prefix)

    # A branch of single characters is a set.
    cset = []
    as_set = True
    for item in items:
        if len(item) != 1:
            as_set = False
            break
        op, av = item[0]
        if op == LITERAL:
            cset.append((op, av))
        elif op == IN and av[0][0] != NEGATE:
            cset.extend(av)
        else:
            as_set = False
            break
    if as_set:
        subpattern.append((IN, _uniq(cset)))
        return subpattern

    subpattern.append((BRANCH, (None, items)))
    return subpattern


def _parse(source, state, verbose, nested, first=False):
    subpattern = _SubPattern(state)
    sourceget = source.get
    sourcematch = source.match

    while True:
        this = source.next
        if this is None:
            break
        if this == "|" or this == ")":
            break
        sourceget()

        if verbose:
            if this in WHITESPACE:
                continue
            if this == "#":
                while True:
                    this = sourceget()
                    if this is None or this == "\n":
                        break
                continue

        if this[0] == "\\":
            subpattern.append(_escape(source, this, state))

        elif this not in SPECIAL_CHARS:
            subpattern.append((LITERAL, ord(this)))

        elif this == "[":
            here = source.tell() - 1
            cset = []
            if source.next == '[':
                _warn('Possible nested set at position %d' % source.tell(), FutureWarning)
            negate = sourcematch("^")
            while True:
                this = sourceget()
                if this is None:
                    raise source.error("unterminated character set", source.tell() - here)
                if this == "]" and cset:
                    break
                elif this[0] == "\\":
                    code1 = _class_escape(source, this)
                else:
                    if cset and this in '-&~|' and source.next == this:
                        _warn('Possible set %s at position %d' % (
                            'difference' if this == '-' else
                            'intersection' if this == '&' else
                            'symmetric difference' if this == '~' else
                            'union',
                            source.tell() - 1), FutureWarning)
                    code1 = LITERAL, ord(this)
                if sourcematch("-"):
                    that = sourceget()
                    if that is None:
                        raise source.error("unterminated character set", source.tell() - here)
                    if that == "]":
                        if code1[0] == IN:
                            code1 = code1[1][0]
                        cset.append(code1)
                        cset.append((LITERAL, 45))
                        break
                    if that[0] == "\\":
                        code2 = _class_escape(source, that)
                    else:
                        if that == '-':
                            _warn('Possible set difference at position %d' % (
                                source.tell() - 2), FutureWarning)
                        code2 = LITERAL, ord(that)
                    if code1[0] != LITERAL or code2[0] != LITERAL:
                        raise source.error(f"bad character range {this}-{that}", len(this) + 1 + len(that))
                    lo = code1[1]
                    hi = code2[1]
                    if hi < lo:
                        raise source.error(f"bad character range {this}-{that}", len(this) + 1 + len(that))
                    cset.append((RANGE, (lo, hi)))
                else:
                    if code1[0] == IN:
                        code1 = code1[1][0]
                    cset.append(code1)

            cset = _uniq(cset)
            if len(cset) == 1 and cset[0][0] == LITERAL:
                if negate:
                    subpattern.append((NOT_LITERAL, cset[0][1]))
                else:
                    subpattern.append(cset[0])
            else:
                if negate:
                    cset.insert(0, (NEGATE, None))
                subpattern.append((IN, cset))

        elif this in REPEAT_CHARS:
            here = source.tell()
            if this == "?":
                min, max = 0, 1
            elif this == "*":
                min, max = 0, MAXREPEAT
            elif this == "+":
                min, max = 1, MAXREPEAT
            else:
                if source.next == "}":
                    subpattern.append((LITERAL, ord(this)))
                    continue
                min, max = 0, MAXREPEAT
                lo = hi = ""
                while source.next in DIGITS:
                    lo += sourceget()
                if sourcematch(","):
                    while source.next in DIGITS:
                        hi += sourceget()
                else:
                    hi = lo
                if not sourcematch("}"):
                    subpattern.append((LITERAL, ord(this)))
                    source.seek(here)
                    continue
                if lo:
                    min = int(lo)
                    if min >= MAXREPEAT:
                        raise OverflowError("the repetition number is too large")
                if hi:
                    max = int(hi)
                    if max >= MAXREPEAT:
                        raise OverflowError("the repetition number is too large")
                    if max < min:
                        raise source.error("min repeat greater than max repeat", source.tell() - here)
            if subpattern:
                item = subpattern[-1:]
            else:
                item = None
            if not item or item[0][0] == AT:
                raise source.error("nothing to repeat", source.tell() - here + len(this))
            if item[0][0] in _REPEATCODES:
                raise source.error("multiple repeat", source.tell() - here + len(this))
            if item[0][0] == SUBPATTERN:
                group, add_flags, del_flags, p = item[0][1]
                if group is None and not add_flags and not del_flags:
                    item = p
            if sourcematch("?"):
                subpattern[-1] = (MIN_REPEAT, (min, max, item))
            elif sourcematch("+"):
                subpattern[-1] = (POSSESSIVE_REPEAT, (min, max, item))
            else:
                subpattern[-1] = (MAX_REPEAT, (min, max, item))

        elif this == ".":
            subpattern.append((ANY, None))

        elif this == "(":
            start = source.tell() - 1
            capture = True
            atomic = False
            name = None
            add_flags = 0
            del_flags = 0
            if sourcematch("?"):
                char = sourceget()
                if char is None:
                    raise source.error("unexpected end of pattern")
                if char == "P":
                    if sourcematch("<"):
                        name = source.getuntil(">", "group name")
                        source.checkgroupname(name, 1)
                    elif sourcematch("="):
                        name = source.getuntil(")", "group name")
                        source.checkgroupname(name, 1)
                        gid = state.groupdict.get(name)
                        if gid is None:
                            raise source.error(f"unknown group name {name!r}", len(name) + 1)
                        if not state.checkgroup(gid):
                            raise source.error("cannot refer to an open group", len(name) + 1)
                        state.checklookbehindgroup(gid, source)
                        subpattern.append((GROUPREF, gid))
                        continue
                    else:
                        char = sourceget()
                        if char is None:
                            raise source.error("unexpected end of pattern")
                        raise source.error("unknown extension ?P" + char, len(char) + 2)
                elif char == ":":
                    capture = False
                elif char == "#":
                    while True:
                        if source.next is None:
                            raise source.error("missing ), unterminated comment", source.tell() - start)
                        if sourceget() == ")":
                            break
                    continue
                elif char in "=!<":
                    dir = 1
                    if char == "<":
                        char = sourceget()
                        if char is None:
                            raise source.error("unexpected end of pattern")
                        if char not in "=!":
                            raise source.error("unknown extension ?<" + char, len(char) + 2)
                        dir = -1
                        lookbehindgroups = state.lookbehindgroups
                        if lookbehindgroups is None:
                            state.lookbehindgroups = state.groups
                    p = _parse_sub(source, state, verbose, nested + 1)
                    if dir < 0:
                        if lookbehindgroups is None:
                            state.lookbehindgroups = None
                    if not sourcematch(")"):
                        raise source.error("missing ), unterminated subpattern", source.tell() - start)
                    if char == "=":
                        subpattern.append((ASSERT, (dir, p)))
                    elif p:
                        subpattern.append((ASSERT_NOT, (dir, p)))
                    else:
                        subpattern.append((FAILURE, ()))
                    continue
                elif char == "(":
                    condname = source.getuntil(")", "group name")
                    if not _is_ascii_decimal(condname):
                        source.checkgroupname(condname, 1)
                        condgroup = state.groupdict.get(condname)
                        if condgroup is None:
                            raise source.error(f"unknown group name {condname!r}", len(condname) + 1)
                    else:
                        condgroup = int(condname)
                        if not condgroup:
                            raise source.error("bad group number", len(condname) + 1)
                        if condgroup >= MAXGROUPS:
                            raise source.error(f"invalid group reference {condgroup}", len(condname) + 1)
                        if condgroup not in state.grouprefpos:
                            state.grouprefpos[condgroup] = source.tell() - len(condname) - 1
                    state.checklookbehindgroup(condgroup, source)
                    item_yes = _parse(source, state, verbose, nested + 1)
                    if source.match("|"):
                        item_no = _parse(source, state, verbose, nested + 1)
                        if source.next == "|":
                            raise source.error("conditional backref with more than two branches")
                    else:
                        item_no = None
                    if not source.match(")"):
                        raise source.error("missing ), unterminated subpattern", source.tell() - start)
                    subpattern.append((GROUPREF_EXISTS, (condgroup, item_yes, item_no)))
                    continue
                elif char == ">":
                    capture = False
                    atomic = True
                elif char in FLAGS or char == "-":
                    flags = _parse_flags(source, state, char)
                    if flags is None:
                        if not first or subpattern:
                            raise source.error('global flags not at the start of the expression',
                                               source.tell() - start)
                        verbose = state.flags & _VERBOSE
                        continue
                    add_flags, del_flags = flags
                    capture = False
                else:
                    raise source.error("unknown extension ?" + char, len(char) + 1)

            if capture:
                try:
                    group = state.opengroup(name)
                except error as err:
                    raise source.error(err.msg, len(name) + 1) from None
            else:
                group = None
            sub_verbose = ((verbose or (add_flags & _VERBOSE)) and
                           not (del_flags & _VERBOSE))
            p = _parse_sub(source, state, sub_verbose, nested + 1)
            if not source.match(")"):
                raise source.error("missing ), unterminated subpattern", source.tell() - start)
            if group is not None:
                state.closegroup(group, p)
            if atomic:
                subpattern.append((ATOMIC_GROUP, p))
            else:
                subpattern.append((SUBPATTERN, (group, add_flags, del_flags, p)))

        elif this == "^":
            subpattern.append((AT, AT_BEGINNING))

        elif this == "$":
            subpattern.append((AT, AT_END))

    # Unpack non-capturing groups.
    i = len(subpattern.data) - 1
    while i >= 0:
        op, av = subpattern.data[i]
        if op == SUBPATTERN:
            group, add_flags, del_flags, p = av
            if group is None and not add_flags and not del_flags:
                subpattern.data[i: i + 1] = p.data
        i -= 1

    return subpattern


def _parse_flags(source, state, char):
    sourceget = source.get
    add_flags = 0
    del_flags = 0
    if char != "-":
        while True:
            flag = FLAGS[char]
            if source.istext:
                if char == 'L':
                    raise source.error("bad inline flags: cannot use 'L' flag with a str pattern")
            else:
                if char == 'u':
                    raise source.error("bad inline flags: cannot use 'u' flag with a bytes pattern")
            add_flags |= flag
            if (flag & TYPE_FLAGS) and (add_flags & TYPE_FLAGS) != flag:
                raise source.error("bad inline flags: flags 'a', 'u' and 'L' are incompatible")
            char = sourceget()
            if char is None:
                raise source.error("missing -, : or )")
            if char in ")-:":
                break
            if char not in FLAGS:
                msg = "unknown flag" if char.isalpha() else "missing -, : or )"
                raise source.error(msg, len(char))
    if char == ")":
        state.flags |= add_flags
        return None
    if add_flags & GLOBAL_FLAGS:
        raise source.error("bad inline flags: cannot turn on global flag", 1)
    if char == "-":
        char = sourceget()
        if char is None:
            raise source.error("missing flag")
        if char not in FLAGS:
            msg = "unknown flag" if char.isalpha() else "missing flag"
            raise source.error(msg, len(char))
        while True:
            flag = FLAGS[char]
            if flag & TYPE_FLAGS:
                raise source.error("bad inline flags: cannot turn off flags 'a', 'u' and 'L'")
            del_flags |= flag
            char = sourceget()
            if char is None:
                raise source.error("missing :")
            if char == ":":
                break
            if char not in FLAGS:
                msg = "unknown flag" if char.isalpha() else "missing :"
                raise source.error(msg, len(char))
    if del_flags & GLOBAL_FLAGS:
        raise source.error("bad inline flags: cannot turn off global flag", 1)
    if add_flags & del_flags:
        raise source.error("bad inline flags: flag turned on and off", 1)
    return add_flags, del_flags


def _fix_flags(src, flags):
    if isinstance(src, str):
        if flags & _LOCALE:
            raise ValueError("cannot use LOCALE flag with a str pattern")
        if not flags & _ASCII:
            flags |= _UNICODE
        elif flags & _UNICODE:
            raise ValueError("ASCII and UNICODE flags are incompatible")
    else:
        if flags & _UNICODE:
            raise ValueError("cannot use UNICODE flag with a bytes pattern")
        if flags & _LOCALE and flags & _ASCII:
            raise ValueError("ASCII and LOCALE flags are incompatible")
    return flags


def _parse_pattern(string, flags):
    source = _Tokenizer(string)
    state = _State()
    state.flags = flags
    p = _parse_sub(source, state, flags & _VERBOSE, 0)
    p.state.flags = _fix_flags(string, p.state.flags)
    if source.next is not None:
        raise source.error("unbalanced parenthesis")
    for g in p.state.grouprefpos:
        if g >= p.state.groups:
            raise error(f"invalid group reference {g}", string, p.state.grouprefpos[g])
    return p


def _parse_template(source, pattern):
    s = _Tokenizer(source)
    sget = s.get
    result = []
    literal = []

    def addliteral():
        if s.istext:
            result.append(''.join(literal))
        else:
            result.append(''.join(literal).encode('latin-1'))
        del literal[:]

    def addgroup(index, pos):
        if index > pattern.groups:
            raise s.error(f"invalid group reference {index}", pos)
        addliteral()
        result.append(index)

    groupindex = pattern.groupindex
    while True:
        this = sget()
        if this is None:
            break
        if this[0] == "\\":
            c = this[1]
            if c == "g":
                if not s.match("<"):
                    raise s.error("missing <")
                name = s.getuntil(">", "group name")
                if not _is_ascii_decimal(name):
                    s.checkgroupname(name, 1)
                    if name not in groupindex:
                        raise IndexError(f"unknown group name {name!r}")
                    index = groupindex[name]
                else:
                    index = int(name)
                    if index >= MAXGROUPS:
                        raise s.error(f"invalid group reference {index}", len(name) + 1)
                addgroup(index, len(name) + 1)
            elif c == "0":
                if s.next in OCTDIGITS:
                    this += sget()
                    if s.next in OCTDIGITS:
                        this += sget()
                literal.append(chr(int(this[1:], 8) & 0xff))
            elif c in DIGITS:
                isoctal = False
                if s.next in DIGITS:
                    this += sget()
                    if (c in OCTDIGITS and this[2] in OCTDIGITS and
                            s.next in OCTDIGITS):
                        this += sget()
                        isoctal = True
                        c = int(this[1:], 8)
                        if c > 0o377:
                            raise s.error(f'octal escape value {this} outside of range 0-0o377', len(this))
                        literal.append(chr(c))
                if not isoctal:
                    addgroup(int(this[1:]), len(this) - 1)
            else:
                if this in ESCAPES:
                    this = chr(ESCAPES[this][1])
                elif c in ASCIILETTERS:
                    raise s.error('bad escape ' + this, len(this))
                literal.append(this)
        else:
            literal.append(this)
    addliteral()
    return result

# ---------------------------------------------------------- code generation

# Opcodes and set items of the native matcher (re.rs).
_FAIL = 0
_MATCH = 1
_CHAR = 2
_NOT_CHAR = 3
_CHAR_I = 4
_NOT_CHAR_I = 5
_ANY = 6
_ANY_ALL = 7
_SET = 8
_AT = 9
_JMP = 10
_SPLIT = 11
_SAVE = 12
_REP1 = 13
_REP_INIT = 14
_REP_UNTIL = 15
_ATOMIC = 16
_LOOK = 17
_END = 18
_GROUPREF = 19
_GROUP_EXISTS = 20

_S_END = 0
_S_LIT = 1
_S_RANGE = 2
_S_CAT = 3
_S_NEGATE = 4

_MAGIC = 0x52454D31


def _combine_flags(flags, add_flags, del_flags):
    if add_flags & TYPE_FLAGS:
        flags &= ~TYPE_FLAGS
    return (flags | add_flags) & ~del_flags


def _simple(p):
    if len(p) != 1:
        return False
    op, av = p[0]
    if op == SUBPATTERN:
        return av[0] is None and _simple(av[-1])
    return op in (LITERAL, NOT_LITERAL, ANY, IN)


def _fold_mode(flags):
    if not flags & _IGNORECASE:
        return 0
    if flags & _UNICODE:
        return 2
    return 1


def _iscased(c, mode):
    lo, up = _fold(c, mode == 2)
    return lo != c or up != c


def _gen_set(code, items, flags):
    mode = _fold_mode(flags)
    if mode:
        cased = False
        for op, av in items:
            if op == LITERAL:
                cased = cased or _iscased(av, mode)
            elif op == RANGE:
                cased = True
        if not cased:
            mode = 0
    uni = 8 if flags & _UNICODE else 0
    at = len(code)
    code.append(_SET)
    code.append(mode)
    code.append(0)
    for op, av in items:
        if op == NEGATE:
            code.append(_S_NEGATE)
        elif op == LITERAL:
            code.append(_S_LIT)
            code.append(av)
        elif op == RANGE:
            code.append(_S_RANGE)
            code.append(av[0])
            code.append(av[1])
        elif op == CATEGORY:
            code.append(_S_CAT)
            code.append(av | uni)
    code.append(_S_END)
    code[at + 2] = len(code) - at


def _gen(code, p, flags, st):
    emit = code.append
    for op, av in p.data:
        if op == LITERAL or op == NOT_LITERAL:
            mode = _fold_mode(flags)
            if mode and _iscased(av, mode):
                emit(_CHAR_I if op == LITERAL else _NOT_CHAR_I)
                emit(av)
                emit(mode)
            else:
                emit(_CHAR if op == LITERAL else _NOT_CHAR)
                emit(av)
        elif op == IN:
            _gen_set(code, av, flags)
        elif op == ANY:
            emit(_ANY_ALL if flags & _DOTALL else _ANY)
        elif op in _REPEATCODES:
            lo, hi, item = av
            if _simple(item):
                kind = 0 if op == MAX_REPEAT else (1 if op == MIN_REPEAT else 2)
                at = len(code)
                emit(_REP1)
                emit(kind)
                emit(lo)
                emit(hi)
                emit(0)
                _gen(code, item, flags, st)
                code[at + 4] = len(code)
            else:
                if op == POSSESSIVE_REPEAT:
                    emit(_ATOMIC)
                r = st[0]
                st[0] = r + 1
                at = len(code)
                emit(_REP_INIT)
                emit(r)
                emit(0)
                body = len(code)
                _gen(code, item, flags, st)
                code[at + 2] = len(code)
                emit(_REP_UNTIL)
                emit(r)
                emit(lo)
                emit(hi)
                emit(1 if op == MIN_REPEAT else 0)
                emit(body)
                if op == POSSESSIVE_REPEAT:
                    emit(_END)
        elif op == SUBPATTERN:
            group, add_flags, del_flags, sub = av
            if group:
                emit(_SAVE)
                emit(2 * group)
            _gen(code, sub, _combine_flags(flags, add_flags, del_flags), st)
            if group:
                emit(_SAVE)
                emit(2 * group + 1)
        elif op == ATOMIC_GROUP:
            emit(_ATOMIC)
            _gen(code, av, flags, st)
            emit(_END)
        elif op == FAILURE:
            emit(_FAIL)
        elif op == ASSERT or op == ASSERT_NOT:
            direction, sub = av
            kind = 1 if op == ASSERT_NOT else 0
            width = 0
            if direction < 0:
                lo, hi = sub.getwidth()
                if lo > MAXREPEAT:
                    raise error("looks too much behind")
                if lo != hi:
                    raise error("look-behind requires fixed-width pattern")
                width = lo
                kind |= 2
            at = len(code)
            emit(_LOOK)
            emit(kind)
            emit(width)
            emit(0)
            _gen(code, sub, flags, st)
            emit(_END)
            code[at + 3] = len(code)
        elif op == AT:
            uni = flags & _UNICODE
            if av == AT_BEGINNING:
                k = 1 if flags & _MULTILINE else 0
            elif av == AT_BEGINNING_STRING:
                k = 0
            elif av == AT_END:
                k = 3 if flags & _MULTILINE else 2
            elif av == AT_END_STRING:
                k = 4
            elif av == AT_BOUNDARY:
                k = 7 if uni else 5
            else:
                k = 8 if uni else 6
            emit(_AT)
            emit(k)
        elif op == BRANCH:
            alts = av[1]
            jumps = []
            last = len(alts) - 1
            for i, alt in enumerate(alts):
                split = -1
                if i < last:
                    split = len(code)
                    emit(_SPLIT)
                    emit(split + 3)
                    emit(0)
                _gen(code, alt, flags, st)
                if i < last:
                    emit(_JMP)
                    jumps.append(len(code))
                    emit(0)
                    code[split + 2] = len(code)
            for j in jumps:
                code[j] = len(code)
        elif op == GROUPREF:
            emit(_GROUPREF)
            emit(av)
            emit(_fold_mode(flags))
        elif op == GROUPREF_EXISTS:
            group, yes, no = av
            at = len(code)
            emit(_GROUP_EXISTS)
            emit(group)
            emit(0)
            _gen(code, yes, flags, st)
            if no is not None:
                emit(_JMP)
                j = len(code)
                emit(0)
                code[at + 2] = len(code)
                _gen(code, no, flags, st)
                code[j] = len(code)
            else:
                code[at + 2] = len(code)
        else:
            raise error(f"internal: unsupported operand type {op!r}")


def _codegen(p, text):
    st = [0]
    code = [_MAGIC, p.state.groups, 0, 1 if text else 0]
    _gen(code, p, p.state.flags, st)
    code.append(_MATCH)
    code[2] = st[0]
    return _pack(code)

# ------------------------------------------------------------------ objects


def _index(x):
    if type(x) is int:
        return x
    if isinstance(x, int):
        return int(x)
    return range(x).stop


def _type_name(x):
    return type(x).__name__


class Pattern:
    def __init__(self, pattern, flags, code, groups, groupindex):
        self.pattern = pattern
        self.flags = flags
        self._code = code
        self.groups = groups
        self.groupindex = groupindex
        self._stype = type(pattern)

    def __repr__(self):
        flags = self.flags
        if isinstance(self.pattern, str):
            flags &= ~_UNICODE
        text = repr(self.pattern)
        if len(text) > 200:
            text = text[:200]
        if not flags:
            return f're.compile({text})'
        return f're.compile({text}, {_flags_text(flags)})'

    def __eq__(self, other):
        if not isinstance(other, Pattern):
            return NotImplemented
        return (self.flags == other.flags and type(self.pattern) is type(other.pattern)
                and self.pattern == other.pattern)

    def __ne__(self, other):
        r = self.__eq__(other)
        if r is NotImplemented:
            return r
        return not r

    def __hash__(self):
        return hash((self.pattern, self.flags))

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        return self

    def _check(self, string):
        if isinstance(string, str):
            if not isinstance(self.pattern, str):
                raise TypeError("cannot use a bytes pattern on a string-like object")
        elif isinstance(string, bytes):
            if isinstance(self.pattern, str):
                raise TypeError("cannot use a string pattern on a bytes-like object")
        else:
            raise TypeError(f"expected string or bytes-like object, got {_type_name(string)!r}")

    def _run(self, string, pos, endpos, mode, limit):
        if type(string) is not type(self.pattern):
            self._check(string)
        if type(pos) is not int:
            pos = _index(pos)
        if type(endpos) is not int:
            endpos = _index(endpos)
        # Mode 13 (search, as `re.Match` objects) needs the last three.
        st = [None, [], False, 0, 0, self, pos, endpos]
        while not _exec(self._code, string, pos, endpos, mode, limit, st):
            pass
        return st[1]

    def _one(self, string, pos, endpos, mode):
        if type(string) is not type(self.pattern):
            self._check(string)
        if type(pos) is not int:
            pos = _index(pos)
        if type(endpos) is not int:
            endpos = _index(endpos)
        found = self._run(string, pos, endpos, mode | 12, 1)
        return found[0] if found else None

    # `_match` (mm-engine's re.rs) runs these natively, and continues in
    # `_match_py` when it cannot.
    def match(self, string, pos=0, endpos=_MAXSIZE):
        return _match(self._code, self, string, pos, endpos, 0)

    def search(self, string, pos=0, endpos=_MAXSIZE):
        return _match(self._code, self, string, pos, endpos, 1)

    def fullmatch(self, string, pos=0, endpos=_MAXSIZE):
        return _match(self._code, self, string, pos, endpos, 2)

    def findall(self, string, pos=0, endpos=_MAXSIZE):
        return self._run(string, pos, endpos, 5, 0)

    def finditer(self, string, pos=0, endpos=_MAXSIZE):
        self._check(string)
        return self._iter(string, _index(pos), _index(endpos))

    def _iter(self, string, pos, endpos):
        # Matches come in growing batches: lazy for a first match, cheap
        # per match for many.
        st = [None, [], False, 0, 0, self, pos, endpos]
        at = pos
        batch = 1
        while True:
            st[1] = []
            while not _exec(self._code, string, at, endpos, 13, batch, st):
                pass
            found = st[1]
            yield from found
            if len(found) < batch:
                return
            m = found[-1]
            at = m.end()
            st[2] = m.start() == at
            if batch < 256:
                batch *= 4

    def scanner(self, string, pos=0, endpos=_MAXSIZE):
        return _Scanner(self, string, pos, endpos)

    def split(self, string, maxsplit=0):
        maxsplit = _index(maxsplit)
        self._check(string)
        if maxsplit < 0:
            return [string]
        n = self.groups
        if n == 0:
            return self._run(string, 0, _MAXSIZE, 9, maxsplit)
        found = self._run(string, 0, _MAXSIZE, 1, maxsplit)
        out = []
        last = 0
        for r in found:
            out.append(string[last:r[1]])
            for i in range(1, n + 1):
                s = r[2 * i + 1]
                out.append(None if s < 0 else string[s:r[2 * i + 2]])
            last = r[2]
        out.append(string[last:])
        return out

    def sub(self, *args, **kwargs):
        repl, string, count = _sub_args('sub', args, kwargs)
        return self._subx(repl, string, count)[0]

    def subn(self, *args, **kwargs):
        repl, string, count = _sub_args('subn', args, kwargs)
        return self._subx(repl, string, count)

    def _subx(self, repl, string, count):
        if callable(repl):
            func = repl
            tmpl = None
        else:
            func = None
            tmpl = _compile_template(self, repl)
        self._check(string)
        count = _index(count)
        if count < 0:
            return string, 0
        if count > 4294967295:
            count = 0
        if func is None and len(tmpl) == 1:
            segs = self._run(string, 0, _MAXSIZE, 9, count)
            return tmpl[0].join(segs), len(segs) - 1
        found = self._run(string, 0, _MAXSIZE, 1 if func is None else 13, count)
        if not found:
            return string, 0
        pieces = []
        last = 0
        empty = string[:0]
        for r in found:
            if func is not None:
                start, end = r.span()
                pieces.append(string[last:start])
                piece = func(r)
                if piece:
                    pieces.append(piece)
                last = end
                continue
            pieces.append(string[last:r[1]])
            if len(tmpl) == 1:
                pieces.append(tmpl[0])
            else:
                pieces.append(_expand(tmpl, string, r, empty))
            last = r[2]
        pieces.append(string[last:])
        return empty.join(pieces), len(found)

    __class_getitem__ = classmethod(list.__class_getitem__.__func__)


def _expand(tmpl, string, r, empty):
    out = [tmpl[0]]
    i = 1
    while i < len(tmpl):
        g = tmpl[i]
        s = r[2 * g + 1]
        if s >= 0:
            out.append(string[s:r[2 * g + 2]])
        out.append(tmpl[i + 1])
        i += 2
    return empty.join(out)


def _sub_args(name, args, kwargs):
    names = ('repl', 'string', 'count')
    given = len(args) + len(kwargs)
    if given > 3:
        raise TypeError(f"{name}() takes at most 3 arguments ({given} given)")
    vals = list(args)
    for k in kwargs:
        if k not in names:
            raise TypeError(f"{name}() got an unexpected keyword argument {k!r}")
        i = names.index(k)
        if i < len(args):
            raise TypeError(f"argument for {name}() given by name ({k!r}) and position ({i + 1})")
    while len(vals) < 3:
        k = names[len(vals)]
        if k in kwargs:
            vals.append(kwargs[k])
        elif k == 'count':
            vals.append(0)
        else:
            raise TypeError(f"{name}() missing required argument {k!r} (pos {len(vals) + 1})")
    return vals[0], vals[1], vals[2]


class _Scanner:
    def __init__(self, pattern, string, pos, endpos):
        pattern._check(string)
        self.pattern = pattern
        self._string = string
        self._pos = self._start = _index(pos)
        self._endpos = _index(endpos)
        self._advance = False

    def _step(self, mode):
        st = [None, [], self._advance, 0, 0, self.pattern, self._start, self._endpos]
        while not _exec(self.pattern._code, self._string, self._pos, self._endpos, mode | 12, 1, st):
            pass
        if not st[1]:
            return None
        m = st[1][0]
        self._pos = m.end()
        self._advance = m.start() == self._pos
        return m

    def match(self):
        return self._step(0)

    def search(self):
        return self._step(1)


def _match_py(code, p, string, pos, endpos, mode, st):
    # `_match` handed over: `st` is the state its first slice left, or
    # `None` for arguments it does not take.
    if st is None:
        return p._one(string, pos, endpos, mode)
    while not _exec(code, string, pos, endpos, mode | 12, 1, st):
        pass
    found = st[1]
    return found[0] if found else None


# `Match` is mm-engine's `re.Match` (re.rs); its methods continue here for
# group indices that may define `__index__`, and for `expand`.

def _gindex(m, g):
    if hasattr(type(g), '__index__'):
        i = type(g).__index__(g)
    else:
        i = m.re.groupindex.get(g, -1)
    if 0 <= i <= m.re.groups:
        return i
    raise IndexError("no such group")


def _Match_group(a):
    m = a[0]
    r = tuple([m.group(_gindex(m, g)) for g in a[1:]])
    return r[0] if len(r) == 1 else r


def _Match___getitem__(a):
    return a[0].group(_gindex(a[0], a[1]))


def _Match_start(a):
    return a[0].start(_gindex(a[0], a[1]))


def _Match_end(a):
    return a[0].end(_gindex(a[0], a[1]))


def _Match_span(a):
    return a[0].span(_gindex(a[0], a[1]))


def _Match_expand(a):
    m = a[0]
    r = (m.lastindex,)
    for span in m.regs:
        r += span
    return _expand(_compile_template(m.re, a[1]), m.string, r, m.string[:0])


def _opt_arg(name, args, kwargs):
    if len(args) + len(kwargs) > 1:
        raise TypeError(f"{name}() takes at most 1 argument ({len(args) + len(kwargs)} given)")
    for k in kwargs:
        if k != 'default':
            raise TypeError(f"{name}() got an unexpected keyword argument {k!r}")
        return kwargs[k]
    if args:
        return args[0]
    return None

# ------------------------------------------------------------ module surface

_cache = {}
_MAXCACHE = 512
_tcache = {}


def _compile(pattern, flags):
    if isinstance(flags, RegexFlag):
        flags = int(flags)
    key = (type(pattern), pattern, flags)
    p = _cache.get(key)
    if p is not None:
        return p
    if isinstance(pattern, Pattern):
        if flags:
            raise ValueError("cannot process flags argument with a compiled pattern")
        return pattern
    if not isinstance(pattern, (str, bytes)):
        raise TypeError("first argument must be string or compiled pattern")
    tree = _parse_pattern(pattern, flags)
    code = _codegen(tree, isinstance(pattern, str))
    groupindex = dict(tree.state.groupdict)
    p = Pattern(pattern, tree.state.flags, code, tree.state.groups - 1, groupindex)
    if len(_cache) >= _MAXCACHE:
        del _cache[next(iter(_cache))]
    _cache[key] = p
    return p


def _compile_template(pattern, repl):
    key = (pattern, type(repl), repl)
    t = _tcache.get(key)
    if t is None:
        t = _parse_template(repl, pattern)
        if len(_tcache) >= _MAXCACHE:
            del _tcache[next(iter(_tcache))]
        _tcache[key] = t
    return t


def compile(pattern, flags=0):
    return _compile(pattern, flags)


def match(pattern, string, flags=0):
    return _compile(pattern, flags).match(string)


def fullmatch(pattern, string, flags=0):
    return _compile(pattern, flags).fullmatch(string)


def search(pattern, string, flags=0):
    return _compile(pattern, flags).search(string)


def findall(pattern, string, flags=0):
    return _compile(pattern, flags).findall(string)


def finditer(pattern, string, flags=0):
    return _compile(pattern, flags).finditer(string)


_zero = object()


def _sub_flags(name, args, count, flags, most):
    if args:
        if count is not _zero:
            raise TypeError(f"{name}() got multiple values for argument 'count'")
        count = args[0]
        args = args[1:]
        if args:
            if flags is not _zero:
                raise TypeError(f"{name}() got multiple values for argument 'flags'")
            flags = args[0]
            args = args[1:]
            if args:
                raise TypeError(f"{name}() takes from {most - 2} to {most} positional arguments "
                                f"but {most + len(args)} were given")
        _warn("'count' is passed as positional argument", DeprecationWarning)
    if count is _zero:
        count = 0
    if flags is _zero:
        flags = 0
    return count, flags


def sub(pattern, repl, string, *args, count=_zero, flags=_zero):
    count, flags = _sub_flags('sub', args, count, flags, 5)
    return _compile(pattern, flags)._subx(repl, string, count)[0]


def subn(pattern, repl, string, *args, count=_zero, flags=_zero):
    count, flags = _sub_flags('subn', args, count, flags, 5)
    return _compile(pattern, flags)._subx(repl, string, count)


def split(pattern, string, *args, maxsplit=_zero, flags=_zero):
    if args:
        if maxsplit is not _zero:
            raise TypeError("split() got multiple values for argument 'maxsplit'")
        maxsplit = args[0]
        args = args[1:]
        if args:
            if flags is not _zero:
                raise TypeError("split() got multiple values for argument 'flags'")
            flags = args[0]
            args = args[1:]
            if args:
                raise TypeError(f"split() takes from 2 to 4 positional arguments but {4 + len(args)} were given")
        _warn("'maxsplit' is passed as positional argument", DeprecationWarning)
    if maxsplit is _zero:
        maxsplit = 0
    if flags is _zero:
        flags = 0
    return _compile(pattern, flags).split(string, maxsplit)


def purge():
    _cache.clear()
    _tcache.clear()


_special_chars = {}
for _c in '()[]{}?*+-|^$\\.&~# \t\n\r\v\f':
    _special_chars[_c] = '\\' + _c


def _escape_str(s):
    get = _special_chars.get
    return ''.join([get(c, c) for c in s])


def escape(pattern):
    if isinstance(pattern, str):
        return _escape_str(pattern)
    return _escape_str(str(pattern, 'latin1')).encode('latin1')


__all__ = [
    "match", "fullmatch", "search", "sub", "subn", "split",
    "findall", "finditer", "compile", "purge", "escape",
    "error", "Pattern", "Match", "A", "I", "L", "M", "S", "X", "U",
    "ASCII", "IGNORECASE", "LOCALE", "MULTILINE", "DOTALL", "VERBOSE",
    "UNICODE", "NOFLAG", "RegexFlag", "PatternError",
]
