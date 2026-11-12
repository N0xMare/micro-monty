# io: in-memory text and binary streams (StringIO, BytesIO), the part of
# CPython's io that needs no filesystem (docs/spec.md §10: files are host
# functions). Newline translation is not applied.

__all__ = ['StringIO', 'BytesIO', 'UnsupportedOperation', 'SEEK_SET', 'SEEK_CUR', 'SEEK_END']

SEEK_SET = 0
SEEK_CUR = 1
SEEK_END = 2


class UnsupportedOperation(OSError, ValueError):
    __module__ = 'io'


class _Memory:
    # The buffer is a list of pieces joined on demand, so writes are amortized.

    def __init__(self, initial, newline='\n'):
        if initial is None:
            initial = self._empty
        elif not isinstance(initial, self._kind):
            raise TypeError(f'initial_value must be {self._kind.__name__} or None, not '
                            f'{type(initial).__name__}')
        self._buf = initial
        self._pending = []
        self._len = len(initial)
        self._pos = 0
        self.closed = False

    def _value(self):
        self._check()
        if self._pending:
            self._buf = self._buf + self._empty.join(self._pending)
            self._pending = []
        return self._buf

    def _check(self):
        if self.closed:
            raise ValueError(self._closed_error)

    def getvalue(self):
        return self._value()

    def write(self, s):
        self._check()
        if not isinstance(s, self._kind):
            raise TypeError(self._write_error(s))
        if self._pos == self._len:
            self._pending.append(s)
            self._len += len(s)
        else:
            buf = self._value()
            if self._pos > len(buf):
                buf = buf + self._pad * (self._pos - len(buf))
            self._buf = buf[:self._pos] + s + buf[self._pos + len(s):]
            self._len = len(self._buf)
        self._pos += len(s)
        return len(s)

    def writelines(self, lines):
        for line in lines:
            self.write(line)

    def read(self, size=-1):
        buf = self._value()
        if size is None or size < 0:
            end = len(buf)
        else:
            end = min(len(buf), self._pos + size)
        r = buf[self._pos:end]
        self._pos = max(self._pos, end)
        return r

    def readline(self, size=-1):
        buf = self._value()
        nl = self._nl
        i = buf.find(nl, self._pos)
        end = len(buf) if i < 0 else i + 1
        if size is not None and size >= 0:
            end = min(end, self._pos + size)
        r = buf[self._pos:end]
        self._pos = max(self._pos, end)
        return r

    def readlines(self, hint=-1):
        return list(self)

    def __iter__(self):
        self._check()
        return self

    def __next__(self):
        line = self.readline()
        if not line:
            raise StopIteration
        return line

    def seek(self, pos, whence=0):
        self._check()
        if whence == 0:
            if pos < 0:
                raise ValueError(f'Negative seek position {pos}')
            self._pos = pos
        elif whence == 1:
            if pos != 0:
                raise OSError("Can't do nonzero cur-relative seeks")
        elif whence == 2:
            if pos != 0:
                raise OSError("Can't do nonzero end-relative seeks")
            self._pos = len(self._value())
        else:
            raise ValueError(f'Invalid whence ({whence}, should be 0, 1 or 2)')
        return self._pos

    def tell(self):
        self._check()
        return self._pos

    def truncate(self, size=None):
        buf = self._value()
        if size is None:
            size = self._pos
        self._buf = buf[:size]
        self._len = len(self._buf)
        return size

    def close(self):
        self.closed = True

    def readable(self):
        self._check()
        return True

    def writable(self):
        self._check()
        return True

    def seekable(self):
        self._check()
        return True

    def flush(self):
        self._check()

    def isatty(self):
        self._check()
        return False

    def __enter__(self):
        self._check()
        return self

    def __exit__(self, *args):
        self.close()


class StringIO(_Memory):
    """Text I/O implementation using an in-memory buffer."""
    _kind = str
    _empty = ''
    _pad = '\0'
    _nl = '\n'
    _closed_error = 'I/O operation on closed file'

    def __init__(self, initial_value='', newline='\n'):
        super().__init__(initial_value, newline)
        self.newline = newline

    @staticmethod
    def _write_error(s):
        return f'string argument expected, got {type(s).__name__!r}'


def _ssize(x):
    # A seek argument as CPython converts it (`__index__`, C `Py_ssize_t`).
    if isinstance(x, int):
        x = int(x)
    else:
        try:
            f = type(x).__index__
        except AttributeError:
            raise TypeError(f"'{type(x).__name__}' object cannot be interpreted as an integer") from None
        x = f(x)
    if not -9223372036854775808 <= x <= 9223372036854775807:
        raise OverflowError('Python int too large to convert to C ssize_t')
    return x


class BytesIO(_Memory):
    """Buffered I/O implementation using an in-memory bytes buffer."""
    _kind = bytes
    _empty = b''
    _pad = b'\0'
    _nl = b'\n'
    _closed_error = 'I/O operation on closed file.'

    def __init__(self, initial_bytes=b''):
        super().__init__(initial_bytes)

    @staticmethod
    def _write_error(s):
        return f"a bytes-like object is required, not '{type(s).__name__}'"

    def seek(self, pos, whence=0):
        # BytesIO seeks relative to the position or the end too.
        self._check()
        pos = _ssize(pos)
        whence = _ssize(whence)
        if whence == 0:
            if pos < 0:
                raise ValueError(f'negative seek value {pos}')
            self._pos = pos
        elif whence == 1:
            self._pos = max(0, self._pos + pos)
        elif whence == 2:
            self._pos = max(0, len(self._value()) + pos)
        else:
            raise ValueError(f'invalid whence ({whence}, should be 0, 1 or 2)')
        return self._pos
