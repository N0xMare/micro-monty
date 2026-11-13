# contextvars: CPython 3.14's API over the engine's per-task contexts. The
# running context is a dict from ContextVar to value (`_mm.ctx_get()`);
# each asyncio task runs in a copy of its creator's (or the one given to
# `create_task(context=)`), and `Context.run` swaps one in.

from _mm import ctx_get as _ctx_get, ctx_swap as _ctx_swap

__all__ = ('Context', 'ContextVar', 'Token', 'copy_context')


class _Missing:
    def __repr__(self):
        return '<Token.MISSING>'


_MISSING = _Missing()


class Context:
    def __init__(self):
        self._data = {}
        self._entered = False

    def run(self, callable, /, *args, **kwargs):
        if self._entered:
            raise RuntimeError(f'cannot enter context: {self!r} is already entered')
        self._entered = True
        old = _ctx_swap(self._data)
        try:
            return callable(*args, **kwargs)
        finally:
            _ctx_swap(old)
            self._entered = False

    def copy(self):
        return _context(dict(self._data))

    def __repr__(self):
        return f'<_contextvars.Context object at 0x{id(self):x}>'

    def __getitem__(self, var):
        if not isinstance(var, ContextVar):
            raise TypeError(f'a ContextVar key was expected, got {var!r}')
        return self._data[var]

    def get(self, var, default=None):
        if not isinstance(var, ContextVar):
            raise TypeError(f'a ContextVar key was expected, got {var!r}')
        return self._data.get(var, default)

    def __contains__(self, var):
        if not isinstance(var, ContextVar):
            raise TypeError(f'a ContextVar key was expected, got {var!r}')
        return var in self._data

    def __len__(self):
        return len(self._data)

    def __iter__(self):
        return iter(list(self._data))

    def keys(self):
        return list(self._data)

    def values(self):
        return list(self._data.values())

    def items(self):
        return list(self._data.items())

    def __eq__(self, other):
        if not isinstance(other, Context):
            return NotImplemented
        return self._data == other._data

    __hash__ = None


def _context(d):
    c = Context()
    c._data = d
    return c


def copy_context():
    return _context(dict(_ctx_get()))


class Token:
    MISSING = _MISSING

    def __init__(self, ctx, var, old_value):
        self._ctx = ctx
        self._var = var
        self._old_value = old_value
        self._used = False

    @property
    def var(self):
        return self._var

    @property
    def old_value(self):
        return self._old_value

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self._var.reset(self)

    def __class_getitem__(cls, item):
        return cls

    def __repr__(self):
        used = ' used' if self._used else ''
        return f'<Token{used} var={self._var!r} at 0x{id(self):x}>'


class ContextVar:
    def __init__(self, name, *, default=_MISSING):
        if not isinstance(name, str):
            raise TypeError('context variable name must be a str')
        self._name = name
        self._default = default

    @property
    def name(self):
        return self._name

    def get(self, default=_MISSING):
        v = _ctx_get().get(self, _MISSING)
        if v is not _MISSING:
            return v
        if default is not _MISSING:
            return default
        if self._default is not _MISSING:
            return self._default
        raise LookupError(self)

    def set(self, value):
        d = _ctx_get()
        token = Token(d, self, d.get(self, _MISSING))
        d[self] = value
        return token

    def reset(self, token):
        if not isinstance(token, Token):
            raise TypeError(f'expected an instance of Token, got {type(token).__name__!r}')
        if token._used:
            raise RuntimeError(f'{token!r} has already been used once')
        if token._var is not self:
            raise ValueError(f'{token!r} was created by a different ContextVar')
        d = _ctx_get()
        if token._ctx is not d:
            raise ValueError(f'{token!r} was created in a different Context')
        if token._old_value is _MISSING:
            d.pop(self, None)
        else:
            d[self] = token._old_value
        token._used = True

    def __class_getitem__(cls, item):
        return cls

    def __repr__(self):
        d = '' if self._default is _MISSING else f' default={self._default!r}'
        return f'<ContextVar name={self._name!r}{d} at 0x{id(self):x}>'
