# traceback: CPython 3.14's printing and formatting functions over the
# engine's own traceback text (exc.rs), so output matches what micro-monty
# prints for an uncaught exception: `_extract(tb, limit)` gives each frame's
# formatted entry and `_hint(exc, with_tb)` the "Did you mean" CPython
# appends to the last line.
# TracebackException is CPython's, without colors and without the keyword
# typo hints of SyntaxError (they need compile()). Bundled stdlib frames never
# show. Left out: the stack functions (no frame objects), walk_tb and
# clear_frames.

import sys

__all__ = ['extract_tb', 'format_exc', 'format_exception', 'format_exception_only',
           'format_list', 'format_tb', 'print_exc', 'print_exception', 'print_last',
           'print_list', 'print_tb', 'FrameSummary', 'StackSummary',
           'TracebackException']

_sentinel = object()

_cause_message = ('\nThe above exception was the direct cause '
                  'of the following exception:\n\n')

_context_message = ('\nDuring handling of the above exception, '
                    'another exception occurred:\n\n')

_RECURSIVE_CUTOFF = 3


def print_list(extracted_list, file=None):
    """Print the list of tuples as returned by extract_tb() or
    extract_stack() as a formatted stack trace to the given file."""
    if file is None:
        file = sys.stderr
    for item in StackSummary.from_list(extracted_list).format():
        print(item, file=file, end="")


def format_list(extracted_list):
    """Format a list of tuples or FrameSummary objects for printing."""
    return StackSummary.from_list(extracted_list).format()


def print_tb(tb, limit=None, file=None):
    """Print up to 'limit' stack trace entries from the traceback 'tb'."""
    print_list(extract_tb(tb, limit=limit), file=file)


def format_tb(tb, limit=None):
    """A shorthand for 'format_list(extract_tb(tb, limit))'."""
    return extract_tb(tb, limit=limit).format()


def extract_tb(tb, limit=None):
    """Return a StackSummary object representing a list of pre-processed
    entries from traceback."""
    summary = StackSummary()
    for filename, lineno, name, line, text in _extract(tb, limit):
        f = FrameSummary(filename, lineno, name, line=line)
        f._text = text
        summary.append(f)
    return summary


def _parse_value_tb(exc, value, tb):
    if (value is _sentinel) != (tb is _sentinel):
        raise ValueError("Both or neither of value and tb must be given")
    if value is tb is _sentinel:
        if exc is not None:
            if isinstance(exc, BaseException):
                return exc, exc.__traceback__
            raise TypeError(f'Exception expected for value, '
                            f'{type(exc).__name__} found')
        else:
            return None, None
    return value, tb


def print_exception(exc, /, value=_sentinel, tb=_sentinel, limit=None,
                    file=None, chain=True, **kwargs):
    """Print exception up to 'limit' stack trace entries from 'tb' to 'file'."""
    value, tb = _parse_value_tb(exc, value, tb)
    te = TracebackException(type(value), value, tb, limit=limit, compact=True)
    te.print(file=file, chain=chain)


def format_exception(exc, /, value=_sentinel, tb=_sentinel, limit=None,
                     chain=True, **kwargs):
    """Format a stack trace and the exception information."""
    value, tb = _parse_value_tb(exc, value, tb)
    te = TracebackException(type(value), value, tb, limit=limit, compact=True)
    return list(te.format(chain=chain))


def format_exception_only(exc, /, value=_sentinel, *, show_group=False, **kwargs):
    """Format the exception part of a traceback."""
    if value is _sentinel:
        value = exc
    te = TracebackException(type(value), value, None, compact=True)
    return list(te.format_exception_only(show_group=show_group))


def _format_final_exc_line(etype, value, *, insert_final_newline=True):
    valuestr = _safe_string(value, 'exception')
    end_char = "\n" if insert_final_newline else ""
    if value is None or not valuestr:
        return f"{etype}{end_char}"
    return f"{etype}: {valuestr}{end_char}"


def _safe_string(value, what, func=str):
    try:
        return func(value)
    except:
        return f'<{what} {func.__name__}() failed>'


def print_exc(limit=None, file=None, chain=True):
    """Shorthand for 'print_exception(sys.exception(), limit=limit, file=file, chain=chain)'."""
    print_exception(sys.exception(), limit=limit, file=file, chain=chain)


def format_exc(limit=None, chain=True):
    """Like print_exc() but return a string."""
    return "".join(format_exception(sys.exception(), limit=limit, chain=chain))


def print_last(limit=None, file=None, chain=True):
    """This is a shorthand for 'print_exception(sys.last_exc, limit=limit, file=file, chain=chain)'."""
    if not hasattr(sys, "last_exc") and not hasattr(sys, "last_type"):
        raise ValueError("no last exception")
    if hasattr(sys, "last_exc"):
        print_exception(sys.last_exc, limit=limit, file=file, chain=chain)
    else:
        print_exception(sys.last_type, sys.last_value, sys.last_traceback,
                        limit=limit, file=file, chain=chain)


class FrameSummary:
    """Information about a single frame from a traceback."""

    def __init__(self, filename, lineno, name, *, lookup_line=True,
                 locals=None, line=None, end_lineno=None, colno=None, end_colno=None, **kwargs):
        self.filename = filename
        self.lineno = lineno
        self.end_lineno = lineno if end_lineno is None else end_lineno
        self.colno = colno
        self.end_colno = end_colno
        self.name = name
        self._line = line
        self._text = None
        self.locals = {k: repr(v) for k, v in locals.items()} if locals else None

    def __eq__(self, other):
        if isinstance(other, FrameSummary):
            return (self.filename == other.filename and
                    self.lineno == other.lineno and
                    self.name == other.name and
                    self.locals == other.locals)
        if isinstance(other, tuple):
            return (self.filename, self.lineno, self.name, self.line) == other
        return NotImplemented

    def __getitem__(self, pos):
        return (self.filename, self.lineno, self.name, self.line)[pos]

    def __iter__(self):
        return iter([self.filename, self.lineno, self.name, self.line])

    def __repr__(self):
        return "<FrameSummary file {filename}, line {lineno} in {name}>".format(
            filename=self.filename, lineno=self.lineno, name=self.name)

    def __len__(self):
        return 4

    @property
    def line(self):
        if self._line is None:
            return None
        return self._line.strip()


class StackSummary(list):
    """A list of FrameSummary objects, representing a stack of frames."""

    @classmethod
    def from_list(klass, a_list):
        result = StackSummary()
        for frame in a_list:
            if isinstance(frame, FrameSummary):
                result.append(frame)
            else:
                filename, lineno, name, line = frame
                result.append(FrameSummary(filename, lineno, name, line=line))
        return result

    def format_frame_summary(self, frame_summary, **kwargs):
        """Format the lines for a single FrameSummary."""
        if frame_summary._text is not None:
            return frame_summary._text
        row = ['  File "{}", line {}, in {}\n'.format(
            frame_summary.filename, frame_summary.lineno, frame_summary.name)]
        if frame_summary.line:
            row.append('    {}\n'.format(frame_summary.line))
        if frame_summary.locals:
            for name, value in sorted(frame_summary.locals.items()):
                row.append('    {name} = {value}\n'.format(name=name, value=value))
        return ''.join(row)

    def format(self, **kwargs):
        """Format the stack ready for printing."""
        result = []
        last_file = None
        last_line = None
        last_name = None
        count = 0
        for frame_summary in self:
            formatted_frame = self.format_frame_summary(frame_summary)
            if formatted_frame is None:
                continue
            if (last_file is None or last_file != frame_summary.filename or
                    last_line is None or last_line != frame_summary.lineno or
                    last_name is None or last_name != frame_summary.name):
                if count > _RECURSIVE_CUTOFF:
                    count -= _RECURSIVE_CUTOFF
                    result.append(
                        f'  [Previous line repeated {count} more '
                        f'time{"s" if count > 1 else ""}]\n'
                    )
                last_file = frame_summary.filename
                last_line = frame_summary.lineno
                last_name = frame_summary.name
                count = 0
            count += 1
            if count > _RECURSIVE_CUTOFF:
                continue
            result.append(formatted_frame)
        if count > _RECURSIVE_CUTOFF:
            count -= _RECURSIVE_CUTOFF
            result.append(
                f'  [Previous line repeated {count} more '
                f'time{"s" if count > 1 else ""}]\n'
            )
        return result


class _ExceptionPrintContext:
    def __init__(self):
        self.seen = set()
        self.exception_group_depth = 0
        self.need_close = False

    def indent(self):
        return ' ' * (2 * self.exception_group_depth)

    def emit(self, text_gen, margin_char=None):
        if margin_char is None:
            margin_char = '|'
        indent_str = self.indent()
        if self.exception_group_depth:
            indent_str += margin_char + ' '

        if isinstance(text_gen, str):
            yield _indent(text_gen, indent_str)
        else:
            for text in text_gen:
                yield _indent(text, indent_str)


def _indent(text, prefix):
    # textwrap.indent(text, prefix, lambda line: True)
    return ''.join(prefix + line for line in text.splitlines(True))


class TracebackException:
    """An exception ready for rendering."""

    def __init__(self, exc_type, exc_value, exc_traceback, *, limit=None,
                 lookup_lines=True, capture_locals=False, compact=False,
                 max_group_width=15, max_group_depth=10, save_exc_type=True,
                 _seen=None):
        is_recursive_call = _seen is not None
        if _seen is None:
            _seen = set()
        _seen.add(id(exc_value))

        self.max_group_width = max_group_width
        self.max_group_depth = max_group_depth

        self.stack = extract_tb(exc_traceback, limit=limit)

        self._exc_type = exc_type if save_exc_type else None

        self._str = _safe_string(exc_value, 'exception')
        try:
            self.__notes__ = getattr(exc_value, '__notes__', None)
        except Exception as e:
            self.__notes__ = [
                f'Ignored error getting __notes__: {_safe_string(e, "__notes__", repr)}']

        self._is_syntax_error = False
        self._have_exc_type = exc_type is not None
        if exc_type is not None:
            self.exc_type_qualname = exc_type.__qualname__
            self.exc_type_module = exc_type.__module__
        else:
            self.exc_type_qualname = None
            self.exc_type_module = None

        if exc_type and issubclass(exc_type, SyntaxError):
            self.filename = exc_value.filename
            lno = exc_value.lineno
            self.lineno = str(lno) if lno is not None else None
            end_lno = exc_value.end_lineno
            self.end_lineno = str(end_lno) if end_lno is not None else None
            self.text = exc_value.text
            self.offset = exc_value.offset
            self.end_offset = exc_value.end_offset
            self.msg = exc_value.msg
            self._is_syntax_error = True
            self._exc_metadata = getattr(exc_value, "_metadata", None)
        elif exc_type and issubclass(exc_type, (NameError, AttributeError, ImportError)):
            hint = _hint(exc_value, exc_traceback is not None)
            if hint:
                self._str += hint
        self.__suppress_context__ = \
            exc_value.__suppress_context__ if exc_value is not None else False

        if not is_recursive_call:
            queue = [(self, exc_value)]
            while queue:
                te, e = queue.pop()
                if (e is not None and e.__cause__ is not None
                        and id(e.__cause__) not in _seen):
                    cause = TracebackException(
                        type(e.__cause__), e.__cause__, e.__cause__.__traceback__,
                        limit=limit, lookup_lines=lookup_lines,
                        capture_locals=capture_locals,
                        max_group_width=max_group_width,
                        max_group_depth=max_group_depth, _seen=_seen)
                else:
                    cause = None

                if compact:
                    need_context = (cause is None and e is not None and
                                    not e.__suppress_context__)
                else:
                    need_context = True
                if (e is not None and e.__context__ is not None
                        and need_context and id(e.__context__) not in _seen):
                    context = TracebackException(
                        type(e.__context__), e.__context__, e.__context__.__traceback__,
                        limit=limit, lookup_lines=lookup_lines,
                        capture_locals=capture_locals,
                        max_group_width=max_group_width,
                        max_group_depth=max_group_depth, _seen=_seen)
                else:
                    context = None

                if e is not None and isinstance(e, BaseExceptionGroup):
                    exceptions = []
                    for exc in e.exceptions:
                        texc = TracebackException(
                            type(exc), exc, exc.__traceback__,
                            limit=limit, lookup_lines=lookup_lines,
                            capture_locals=capture_locals,
                            max_group_width=max_group_width,
                            max_group_depth=max_group_depth, _seen=_seen)
                        exceptions.append(texc)
                else:
                    exceptions = None

                te.__cause__ = cause
                te.__context__ = context
                te.exceptions = exceptions
                if cause:
                    queue.append((te.__cause__, e.__cause__))
                if context:
                    queue.append((te.__context__, e.__context__))
                if exceptions:
                    queue.extend(zip(te.exceptions, e.exceptions))

    @classmethod
    def from_exception(cls, exc, *args, **kwargs):
        """Create a TracebackException from an exception."""
        return cls(type(exc), exc, exc.__traceback__, *args, **kwargs)

    @property
    def exc_type(self):
        import warnings
        warnings.warn('Deprecated in 3.13. Use exc_type_str instead.',
                      DeprecationWarning, stacklevel=2)
        return self._exc_type

    @property
    def exc_type_str(self):
        if not self._have_exc_type:
            return None
        stype = self.exc_type_qualname
        smod = self.exc_type_module
        if smod not in ("__main__", "builtins"):
            if not isinstance(smod, str):
                smod = "<unknown>"
            stype = smod + '.' + stype
        return stype

    def _load_lines(self):
        """Private API. force all lines in the stack to be loaded."""
        for frame in self.stack:
            frame.line

    def __eq__(self, other):
        if isinstance(other, TracebackException):
            return self.__dict__ == other.__dict__
        return NotImplemented

    def __str__(self):
        return self._str

    def format_exception_only(self, *, show_group=False, _depth=0, **kwargs):
        """Format the exception part of the traceback."""
        indent = 3 * _depth * ' '
        if not self._have_exc_type:
            yield indent + _format_final_exc_line(None, self._str)
            return

        stype = self.exc_type_str
        if not self._is_syntax_error:
            if _depth > 0:
                formatted = _format_final_exc_line(
                    stype, self._str, insert_final_newline=False).split('\n')
                yield from [indent + l + '\n' for l in formatted]
            else:
                yield _format_final_exc_line(stype, self._str)
        else:
            yield from [indent + l for l in self._format_syntax_error(stype)]

        notes = self.__notes__
        if isinstance(notes, (list, tuple)):
            for note in notes:
                note = _safe_string(note, 'note')
                yield from [indent + l + '\n' for l in note.split('\n')]
        elif notes is not None:
            yield indent + "{}\n".format(_safe_string(notes, '__notes__', func=repr))

        if self.exceptions and show_group:
            for ex in self.exceptions:
                yield from ex.format_exception_only(show_group=show_group, _depth=_depth + 1)

    def _format_syntax_error(self, stype, **kwargs):
        """Format SyntaxError exceptions (internal helper)."""
        filename_suffix = ''
        if self.lineno is not None:
            yield '  File "{}", line {}\n'.format(self.filename or "<string>", self.lineno)
        elif self.filename is not None:
            filename_suffix = ' ({})'.format(self.filename)

        text = self.text
        if isinstance(text, str):
            rtext = text.rstrip('\n')
            ltext = rtext.lstrip(' \n\f')
            spaces = len(rtext) - len(ltext)
            if self.offset is None:
                yield '    {}\n'.format(ltext)
            elif isinstance(self.offset, int):
                offset = self.offset
                if self.lineno == self.end_lineno:
                    end_offset = (
                        self.end_offset
                        if isinstance(self.end_offset, int) and self.end_offset != 0
                        else offset)
                else:
                    end_offset = len(rtext) + 1
                if self.text and offset > len(self.text):
                    offset = len(rtext) + 1
                if self.text and end_offset > len(self.text):
                    end_offset = len(rtext) + 1
                if offset >= end_offset or end_offset < 0:
                    end_offset = offset + 1
                colno = offset - 1 - spaces
                end_colno = end_offset - 1 - spaces
                yield '    {}\n'.format(ltext)
                if colno >= 0:
                    caretspace = ''.join(c if c.isspace() else ' ' for c in ltext[:colno])
                    yield '    {}{}\n'.format(caretspace, '^' * (end_colno - colno))
        msg = self.msg or "<no detail available>"
        yield "{}: {}{}\n".format(stype, msg, filename_suffix)

    def format(self, *, chain=True, _ctx=None, **kwargs):
        """Format the exception."""
        if _ctx is None:
            _ctx = _ExceptionPrintContext()

        output = []
        exc = self
        if chain:
            while exc:
                if exc.__cause__ is not None:
                    chained_msg = _cause_message
                    chained_exc = exc.__cause__
                elif (exc.__context__ is not None and
                      not exc.__suppress_context__):
                    chained_msg = _context_message
                    chained_exc = exc.__context__
                else:
                    chained_msg = None
                    chained_exc = None

                output.append((chained_msg, exc))
                exc = chained_exc
        else:
            output.append((None, exc))

        for msg, exc in reversed(output):
            if msg is not None:
                yield from _ctx.emit(msg)
            if exc.exceptions is None:
                if exc.stack:
                    yield from _ctx.emit('Traceback (most recent call last):\n')
                    yield from _ctx.emit(exc.stack.format())
                yield from _ctx.emit(exc.format_exception_only())
            elif _ctx.exception_group_depth > self.max_group_depth:
                yield from _ctx.emit(
                    f"... (max_group_depth is {self.max_group_depth})\n")
            else:
                is_toplevel = (_ctx.exception_group_depth == 0)
                if is_toplevel:
                    _ctx.exception_group_depth += 1

                if exc.stack:
                    yield from _ctx.emit(
                        'Exception Group Traceback (most recent call last):\n',
                        margin_char='+' if is_toplevel else None)
                    yield from _ctx.emit(exc.stack.format())

                yield from _ctx.emit(exc.format_exception_only())
                num_excs = len(exc.exceptions)
                if num_excs <= self.max_group_width:
                    n = num_excs
                else:
                    n = self.max_group_width + 1
                _ctx.need_close = False
                for i in range(n):
                    last_exc = (i == n - 1)
                    if last_exc:
                        _ctx.need_close = True

                    if self.max_group_width is not None:
                        truncated = (i >= self.max_group_width)
                    else:
                        truncated = False
                    title = f'{i + 1}' if not truncated else '...'
                    yield (_ctx.indent() +
                           ('+-' if i == 0 else '  ') +
                           f'+---------------- {title} ----------------\n')
                    _ctx.exception_group_depth += 1
                    if not truncated:
                        yield from exc.exceptions[i].format(chain=chain, _ctx=_ctx)
                    else:
                        remaining = num_excs - self.max_group_width
                        plural = 's' if remaining > 1 else ''
                        yield from _ctx.emit(
                            f"and {remaining} more exception{plural}\n")

                    if last_exc and _ctx.need_close:
                        yield (_ctx.indent() +
                               "+------------------------------------\n")
                        _ctx.need_close = False
                    _ctx.exception_group_depth -= 1

                if is_toplevel:
                    _ctx.exception_group_depth = 0

    def print(self, *, file=None, chain=True, **kwargs):
        """Print the result of self.format(chain=chain) to 'file'."""
        if file is None:
            file = sys.stderr
        for line in self.format(chain=chain):
            print(line, file=file, end="")
