# csv: CPython 3.14's Lib/csv.py (DictReader, DictWriter, the dialect
# classes) over a port of Modules/_csv.c's reader and writer state machines,
# with _csv's names and messages. Sniffer is not provided. PSF license; see
# THIRD_PARTY.md.

__all__ = ["QUOTE_MINIMAL", "QUOTE_ALL", "QUOTE_NONNUMERIC", "QUOTE_NONE",
           "QUOTE_STRINGS", "QUOTE_NOTNULL",
           "Error", "Dialect", "excel", "excel_tab",
           "field_size_limit", "reader", "writer",
           "register_dialect", "get_dialect", "list_dialects",
           "unregister_dialect", "DictReader", "DictWriter",
           "unix_dialect"]

__version__ = "1.0"

QUOTE_MINIMAL, QUOTE_ALL, QUOTE_NONNUMERIC, QUOTE_NONE, QUOTE_STRINGS, QUOTE_NOTNULL = range(6)

_dialects = {}
_field_limit = 128 * 1024
_NOT_SET = object()
_KEYWORDS = ('dialect', 'delimiter', 'doublequote', 'escapechar', 'lineterminator',
             'quotechar', 'quoting', 'skipinitialspace', 'strict')


class Error(Exception):
    __module__ = '_csv'


def _clinic(fname, args, kwargs, name):
    # Argument Clinic's one-argument parsing, with its messages.
    n = len(args) + len(kwargs)
    if n > 1:
        raise TypeError(f'{fname}() takes at most 1 argument ({n} given)')
    if args:
        return args[0]
    if name in kwargs:
        return kwargs[name]
    return _NOT_SET


def _char(name, v, default, none_ok):
    if v is _NOT_SET:
        return default
    if v is None and none_ok:
        return _NOT_SET
    what = 'a unicode character or None' if none_ok else 'a unicode character'
    if not isinstance(v, str):
        raise TypeError(f'"{name}" must be {what}, not {type(v).__name__}')
    if len(v) != 1:
        raise TypeError(f'"{name}" must be {what}, not a string of length {len(v)}')
    return v


# _csv's types carry their C names (the Python names below replace them).
class Dialect:
    __module__ = '_csv'
    __mm_tpname__ = '_csv.Dialect'

    def __new__(cls, *args, **kwargs):
        if len(args) > 1:
            raise TypeError(f'function takes at most 1 positional argument ({len(args)} given)')
        for k in kwargs:
            if k not in _KEYWORDS:
                raise TypeError(f"this function got an unexpected keyword argument '{k}'")
        p = dict.fromkeys(_KEYWORDS, _NOT_SET)
        if args:
            if 'dialect' in kwargs:
                raise TypeError("argument for function given by name ('dialect') "
                                'and position (1)')
            p['dialect'] = args[0]
        p.update(kwargs)
        dialect = p['dialect']
        if dialect is not _NOT_SET:
            if isinstance(dialect, str):
                dialect = get_dialect(dialect)
            if type(dialect) is _Dialect and all(k == 'dialect' for k in kwargs):
                return dialect
            for k in _KEYWORDS[1:]:
                if p[k] is _NOT_SET:
                    try:
                        p[k] = getattr(dialect, k)
                    except AttributeError:
                        pass
        self = object.__new__(cls)
        s = object.__setattr__
        delimiter = _char('delimiter', p['delimiter'], ',', False)
        doublequote = True if p['doublequote'] is _NOT_SET else bool(p['doublequote'])
        escapechar = _char('escapechar', p['escapechar'], _NOT_SET, True)
        lt = p['lineterminator']
        if lt is _NOT_SET:
            lt = '\r\n'
        elif not isinstance(lt, str):
            raise TypeError(f'"lineterminator" must be a string, not {type(lt).__name__}')
        quotechar = _char('quotechar', p['quotechar'], '"', True)
        quoting = p['quoting']
        if quoting is _NOT_SET:
            quoting = QUOTE_MINIMAL
        else:
            if type(quoting) is not int:
                raise TypeError(f'"quoting" must be an integer, not {type(quoting).__name__}')
            if not -0x80000000 <= quoting <= 0x7FFFFFFF:
                raise OverflowError('Python int too large to convert to C int')
        skip = False if p['skipinitialspace'] is _NOT_SET else bool(p['skipinitialspace'])
        strict = False if p['strict'] is _NOT_SET else bool(p['strict'])
        if not 0 <= quoting <= QUOTE_NOTNULL:
            raise TypeError('bad "quoting" value')
        if p['quotechar'] is None and p['quoting'] is _NOT_SET:
            quoting = QUOTE_NONE
        if quoting != QUOTE_NONE and quotechar is _NOT_SET:
            raise TypeError('quotechar must be set if quoting enabled')
        for name, c, space_ok in (('delimiter', delimiter, True),
                                  ('escapechar', escapechar, not skip),
                                  ('quotechar', quotechar, not skip)):
            if c is _NOT_SET:
                continue
            if c == '\r' or c == '\n' or (c == ' ' and not space_ok):
                raise ValueError(f'bad {name} value')
            if c in lt:
                raise ValueError(f'bad {name} or lineterminator value')
        for n1, c1, n2, c2 in (('delimiter', delimiter, 'escapechar', escapechar),
                               ('delimiter', delimiter, 'quotechar', quotechar),
                               ('escapechar', escapechar, 'quotechar', quotechar)):
            if c1 is not _NOT_SET and c1 == c2:
                raise ValueError(f'bad {n1} or {n2} value')
        s(self, '_delimiter', delimiter)
        s(self, '_doublequote', doublequote)
        s(self, '_escapechar', escapechar)
        s(self, '_lineterminator', lt)
        s(self, '_quotechar', quotechar)
        s(self, '_quoting', quoting)
        s(self, '_skipinitialspace', skip)
        s(self, '_strict', strict)
        return self

    delimiter = property(lambda self: self._delimiter)
    doublequote = property(lambda self: self._doublequote)
    escapechar = property(lambda self: None if self._escapechar is _NOT_SET else
                          self._escapechar)
    lineterminator = property(lambda self: self._lineterminator)
    quotechar = property(lambda self: None if self._quotechar is _NOT_SET else
                         self._quotechar)
    quoting = property(lambda self: self._quoting)
    skipinitialspace = property(lambda self: self._skipinitialspace)
    strict = property(lambda self: self._strict)

    def __setattr__(self, name, value):
        if name in _KEYWORDS:
            raise AttributeError(f"attribute '{name}' of '_csv.Dialect' objects is not writable")
        raise AttributeError(f"'_csv.Dialect' object has no attribute '{name}'")

    def __getattr__(self, name):
        raise AttributeError(f"'_csv.Dialect' object has no attribute '{name}'")


_Dialect = Dialect


def _call_dialect(name, args, kwargs):
    if not args:
        raise TypeError(f'{name} expected at least 1 argument, got 0')
    if len(args) > 2:
        raise TypeError(f'{name} expected at most 2 arguments, got {len(args)}')
    return args[0], (_Dialect(args[1], **kwargs) if len(args) > 1 else _Dialect(**kwargs))


def _readonly(kind, name):
    if name in ('dialect', 'line_num'):
        raise AttributeError('readonly attribute')
    raise AttributeError(f"'_csv.{kind}' object has no attribute '{name}' and no __dict__ "
                         'for setting new attributes')


# The reader's states (_csv.c's ParserState).
(_START_RECORD, _START_FIELD, _ESCAPED_CHAR, _IN_FIELD, _IN_QUOTED_FIELD,
 _ESCAPE_IN_QUOTED_FIELD, _QUOTE_IN_QUOTED_FIELD, _EAT_CRNL, _AFTER_ESCAPED_CRNL) = range(9)


class reader:
    __module__ = '_csv'
    __mm_tpname__ = '_csv.reader'

    def __iter__(self):
        return self

    def __setattr__(self, name, value):
        _readonly('reader', name)

    def __next__(self):
        st = self._st
        st[0] = []
        st[1] = []
        st[2] = _START_RECORD
        st[3] = True
        while True:
            try:
                line = next(self._input)
            except StopIteration:
                if st[1] or st[2] == _IN_QUOTED_FIELD:
                    if self._dialect._strict:
                        raise Error('unexpected end of data') from None
                    self._save()
                    break
                raise
            if not isinstance(line, str):
                raise Error(f'iterator should return strings, not {type(line).__name__} '
                            '(the file should be opened in text mode)')
            object.__setattr__(self, '_line_num', self._line_num + 1)
            for c in line:
                self._char(c)
            self._char(None)
            if st[2] == _START_RECORD:
                break
        fields = st[0]
        st[0] = None
        return fields

    line_num = property(lambda self: self._line_num)
    dialect = property(lambda self: self._dialect)

    def _save(self):
        st = self._st
        quoting = self._dialect._quoting
        field = ''.join(st[1])
        if st[3] and not field and (quoting == QUOTE_NOTNULL or quoting == QUOTE_STRINGS):
            field = None
        elif st[3] and field and (quoting == QUOTE_NONNUMERIC or quoting == QUOTE_STRINGS):
            field = float(field)
        st[0].append(field)
        st[1] = []

    def _add(self, c):
        st = self._st
        if len(st[1]) >= _field_limit:
            raise Error(f'field larger than field limit ({_field_limit})')
        st[1].append(c)

    def _char(self, c):
        # _csv.c's parse_process_char; `c` None is the end of a line.
        st = self._st
        d = self._dialect
        state = st[2]
        eol = c is None
        crlf = c == '\n' or c == '\r'
        if state == _START_RECORD:
            if eol:
                return
            if crlf:
                st[2] = _EAT_CRNL
                return
            state = _START_FIELD
        if state == _START_FIELD:
            st[3] = True
            if crlf or eol:
                self._save()
                st[2] = _START_RECORD if eol else _EAT_CRNL
            elif c == d._quotechar and d._quoting != QUOTE_NONE:
                st[3] = False
                st[2] = _IN_QUOTED_FIELD
            elif c == d._escapechar:
                st[2] = _ESCAPED_CHAR
            elif c == ' ' and d._skipinitialspace:
                st[2] = _START_FIELD
            elif c == d._delimiter:
                self._save()
                st[2] = _START_FIELD
            else:
                self._add(c)
                st[2] = _IN_FIELD
            return
        if state == _ESCAPED_CHAR:
            if crlf:
                self._add(c)
                st[2] = _AFTER_ESCAPED_CRNL
                return
            self._add('\n' if eol else c)
            st[2] = _IN_FIELD
            return
        if state == _AFTER_ESCAPED_CRNL:
            if eol:
                return
            state = _IN_FIELD
        if state == _IN_FIELD:
            if crlf or eol:
                self._save()
                st[2] = _START_RECORD if eol else _EAT_CRNL
            elif c == d._escapechar:
                st[2] = _ESCAPED_CHAR
            elif c == d._delimiter:
                self._save()
                st[2] = _START_FIELD
            else:
                self._add(c)
                st[2] = _IN_FIELD
        elif state == _IN_QUOTED_FIELD:
            if eol:
                pass
            elif c == d._escapechar:
                st[2] = _ESCAPE_IN_QUOTED_FIELD
            elif c == d._quotechar and d._quoting != QUOTE_NONE:
                st[2] = _QUOTE_IN_QUOTED_FIELD if d._doublequote else _IN_FIELD
            else:
                self._add(c)
        elif state == _ESCAPE_IN_QUOTED_FIELD:
            self._add('\n' if eol else c)
            st[2] = _IN_QUOTED_FIELD
        elif state == _QUOTE_IN_QUOTED_FIELD:
            if d._quoting != QUOTE_NONE and c == d._quotechar:
                self._add(c)
                st[2] = _IN_QUOTED_FIELD
            elif c == d._delimiter:
                self._save()
                st[2] = _START_FIELD
            elif crlf or eol:
                self._save()
                st[2] = _START_RECORD if eol else _EAT_CRNL
            elif not d._strict:
                self._add(c)
                st[2] = _IN_FIELD
            else:
                raise Error(f"'{d._delimiter}' expected after '{d._quotechar}'")
        elif state == _EAT_CRNL:
            if crlf:
                pass
            elif eol:
                st[2] = _START_RECORD
            else:
                raise Error("new-line character seen in unquoted field - do you need "
                            "to open the file with newline=''?")


_Reader = reader


def reader(*args, **kwargs):
    """Return a reader object that iterates over the lines of the CSV `iterable`."""
    if not args:
        raise TypeError('reader expected at least 1 argument, got 0')
    it = iter(args[0])
    _, dialect = _call_dialect('reader', args, kwargs)
    r = object.__new__(_Reader)
    s = object.__setattr__
    s(r, '_input', it)
    s(r, '_dialect', dialect)
    s(r, '_line_num', 0)
    # fields, field characters, state, unquoted field
    s(r, '_st', [None, [], _START_RECORD, True])
    return r


def _is_number(v):
    # PyNumber_Check: the type has a numeric conversion slot, or is complex.
    t = type(v)
    return (isinstance(v, (int, float)) or t.__name__ == 'complex' or
            hasattr(t, '__index__') or hasattr(t, '__int__') or hasattr(t, '__float__'))


class writer:
    __module__ = '_csv'
    __mm_tpname__ = '_csv.writer'

    def __setattr__(self, name, value):
        _readonly('writer', name)

    dialect = property(lambda self: self._dialect)

    def _field(self, out, field, quoted, first):
        # _csv.c's join_append_data, both phases.
        d = self._dialect
        q = d._quotechar
        e = d._escapechar
        buf = []
        for c in field:
            if (c == d._delimiter or c == e or c == q or c == '\n' or c == '\r' or
                    c in d._lineterminator):
                escape = False
                if d._quoting == QUOTE_NONE:
                    escape = True
                else:
                    if c == q:
                        if d._doublequote:
                            buf.append(q)
                        else:
                            escape = True
                    elif c == e:
                        escape = True
                    if not escape:
                        quoted = True
                if escape:
                    if e is _NOT_SET:
                        raise Error('need to escape, but no escapechar set')
                    buf.append(e)
            buf.append(c)
        if not first:
            out.append(d._delimiter)
        if quoted:
            out.append(q)
            out.extend(buf)
            out.append(q)
        else:
            out.extend(buf)

    def writerow(self, *args):
        """Construct and write a CSV record from an iterable of fields."""
        if len(args) != 1:
            raise TypeError(f'writer.writerow() takes exactly one argument ({len(args)} given)')
        try:
            it = iter(args[0])
        except TypeError:
            raise Error(f'iterable expected, not {type(args[0]).__name__}') from None
        d = self._dialect
        quoting = d._quoting
        out = []
        n = 0
        null = False
        for field in it:
            if quoting == QUOTE_NONNUMERIC:
                quoted = not _is_number(field)
            elif quoting == QUOTE_ALL:
                quoted = True
            elif quoting == QUOTE_STRINGS:
                quoted = isinstance(field, str)
            elif quoting == QUOTE_NOTNULL:
                quoted = field is not None
            else:
                quoted = False
            null = field is None
            if null:
                text = ''
            elif isinstance(field, str):
                text = field
            else:
                text = str(field)
            self._field(out, text, quoted, n == 0)
            n += 1
        if n > 0 and not out:
            if quoting == QUOTE_NONE or (null and (quoting == QUOTE_STRINGS or
                                                   quoting == QUOTE_NOTNULL)):
                raise Error('single empty field record must be quoted')
            out.append(d._quotechar)
            out.append(d._quotechar)
        out.append(d._lineterminator)
        return self._write(''.join(out))

    def writerows(self, rows):
        """Construct and write a series of iterables to a csv file."""
        for row in rows:
            self.writerow(row)
        return None


_Writer = writer


def writer(*args, **kwargs):
    """Return a writer object that writes CSV records to `fileobj`."""
    if not args:
        raise TypeError('writer expected at least 1 argument, got 0')
    w = object.__new__(_Writer)
    write = getattr(args[0], 'write', None)
    if write is None or not callable(write):
        raise TypeError('argument 1 must have a "write" method')
    _, dialect = _call_dialect('writer', args, kwargs)
    object.__setattr__(w, '_write', write)
    object.__setattr__(w, '_dialect', dialect)
    return w


def register_dialect(name, dialect=_NOT_SET, **kwargs):
    """Create a mapping from a string name to a dialect class."""
    if not isinstance(name, str):
        raise TypeError('dialect name must be a string')
    d = _Dialect(**kwargs) if dialect is _NOT_SET else _Dialect(dialect, **kwargs)
    _dialects[name] = d


def unregister_dialect(*args, **kwargs):
    """Delete the name/dialect mapping associated with a string name."""
    name = _clinic('unregister_dialect', args, kwargs, 'name')
    if name is _NOT_SET:
        raise TypeError("unregister_dialect() missing required argument 'name' (pos 1)")
    try:
        del _dialects[name]
    except (KeyError, TypeError):
        raise Error('unknown dialect') from None


def get_dialect(*args, **kwargs):
    """Return the dialect instance associated with name."""
    name = _clinic('get_dialect', args, kwargs, 'name')
    if name is _NOT_SET:
        raise TypeError("get_dialect() missing required argument 'name' (pos 1)")
    try:
        return _dialects[name]
    except (KeyError, TypeError):
        raise Error('unknown dialect') from None


def list_dialects():
    """Return a list of all known dialect names."""
    return list(_dialects)


def field_size_limit(*args, **kwargs):
    """Sets an upper limit on parsed fields; returns the old limit."""
    global _field_limit
    n = len(args) + len(kwargs)
    if n > 1:
        raise TypeError(f'field_size_limit() takes at most 1 argument ({n} given)')
    for k in kwargs:
        if k != 'new_limit':
            raise TypeError(f"field_size_limit() got an unexpected keyword argument '{k}'")
    old = _field_limit
    if n:
        new = args[0] if args else kwargs['new_limit']
        if not isinstance(new, int):
            raise TypeError('limit must be an integer')
        if not -0x8000000000000000 <= new <= 0x7FFFFFFFFFFFFFFF:
            raise OverflowError('Python int too large to convert to C long')
        _field_limit = new
    return old


class Dialect:
    """Describe a CSV dialect.

    This must be subclassed (see csv.excel).  Valid attributes are:
    delimiter, quotechar, escapechar, doublequote, skipinitialspace,
    lineterminator, quoting.

    """
    _name = ""
    _valid = False
    # placeholders
    delimiter = None
    quotechar = None
    escapechar = None
    doublequote = None
    skipinitialspace = None
    lineterminator = None
    quoting = None

    def __init__(self):
        if self.__class__ != Dialect:
            self._valid = True
        self._validate()

    def _validate(self):
        try:
            _Dialect(self)
        except TypeError as e:
            # Re-raise to get a traceback showing more user code.
            raise Error(str(e)) from None


class excel(Dialect):
    """Describe the usual properties of Excel-generated CSV files."""
    delimiter = ','
    quotechar = '"'
    doublequote = True
    skipinitialspace = False
    lineterminator = '\r\n'
    quoting = QUOTE_MINIMAL


register_dialect("excel", excel)


class excel_tab(excel):
    """Describe the usual properties of Excel-generated TAB-delimited files."""
    delimiter = '\t'


register_dialect("excel-tab", excel_tab)


class unix_dialect(Dialect):
    """Describe the usual properties of Unix-generated CSV files."""
    delimiter = ','
    quotechar = '"'
    doublequote = True
    skipinitialspace = False
    lineterminator = '\n'
    quoting = QUOTE_ALL


register_dialect("unix", unix_dialect)


class DictReader:
    def __init__(self, f, fieldnames=None, restkey=None, restval=None,
                 dialect="excel", *args, **kwds):
        if fieldnames is not None and iter(fieldnames) is fieldnames:
            fieldnames = list(fieldnames)
        self._fieldnames = fieldnames   # list of keys for the dict
        self.restkey = restkey          # key to catch long rows
        self.restval = restval          # default value for short rows
        self.reader = reader(f, dialect, *args, **kwds)
        self.dialect = dialect
        self.line_num = 0

    def __iter__(self):
        return self

    @property
    def fieldnames(self):
        if self._fieldnames is None:
            try:
                self._fieldnames = next(self.reader)
            except StopIteration:
                pass
        self.line_num = self.reader.line_num
        return self._fieldnames

    @fieldnames.setter
    def fieldnames(self, value):
        self._fieldnames = value

    def __next__(self):
        if self.line_num == 0:
            # Used only for its side effect.
            self.fieldnames
        row = next(self.reader)
        self.line_num = self.reader.line_num

        # unlike the basic reader, we prefer not to return blanks,
        # because we will typically wind up with a dict full of None
        # values
        while row == []:
            row = next(self.reader)
        d = dict(zip(self.fieldnames, row))
        lf = len(self.fieldnames)
        lr = len(row)
        if lf < lr:
            d[self.restkey] = row[lf:]
        elif lf > lr:
            for key in self.fieldnames[lr:]:
                d[key] = self.restval
        return d


class DictWriter:
    def __init__(self, f, fieldnames, restval="", extrasaction="raise",
                 dialect="excel", *args, **kwds):
        if fieldnames is not None and iter(fieldnames) is fieldnames:
            fieldnames = list(fieldnames)
        self.fieldnames = fieldnames    # list of keys for the dict
        self.restval = restval          # for writing short dicts
        extrasaction = extrasaction.lower()
        if extrasaction not in ("raise", "ignore"):
            raise ValueError(f"extrasaction ({extrasaction}) must be 'raise' or 'ignore'")
        self.extrasaction = extrasaction
        self.writer = writer(f, dialect, *args, **kwds)

    def writeheader(self):
        header = dict(zip(self.fieldnames, self.fieldnames))
        return self.writerow(header)

    def _dict_to_list(self, rowdict):
        if self.extrasaction == "raise":
            # rowdict.keys() - self.fieldnames
            wrong_fields = set(rowdict.keys())
            wrong_fields.difference_update(self.fieldnames)
            if wrong_fields:
                raise ValueError("dict contains fields not in fieldnames: "
                                 + ", ".join([repr(x) for x in wrong_fields]))
        return (rowdict.get(key, self.restval) for key in self.fieldnames)

    def writerow(self, rowdict):
        return self.writer.writerow(self._dict_to_list(rowdict))

    def writerows(self, rowdicts):
        return self.writer.writerows(map(self._dict_to_list, rowdicts))
