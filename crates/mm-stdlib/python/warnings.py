# warnings: CPython 3.14's Lib/_py_warnings.py (PSF; THIRD_PARTY.md) without
# the C accelerator, threads, context-aware filters, -W options or linecache.
# There are no frame objects: warn() finds its caller's filename, line and
# globals with _mm.caller, which walks the VM's frames as CPython's C
# _warnings walks f_back (prelude frames are skipped, as CPython's C code
# has none). catch_warnings ignores its module argument.

import sys
from _mm import caller as _caller

__all__ = ["warn", "warn_explicit", "showwarning",
           "formatwarning", "filterwarnings", "simplefilter",
           "resetwarnings", "catch_warnings", "deprecated"]

# filters contains a sequence of filter 5-tuples: an action, a compiled
# regex that must match the message (or None), a category, a compiled regex
# (or exact string) that must match the module (or None), and a line number
# (0 matches any line).
filters = []

defaultaction = "default"
onceregistry = {}
_filters_version = 1
_registry = {}

_ACTIONS = ("error", "ignore", "always", "all", "default", "module", "once")


def _filters_mutated():
    global _filters_version
    _filters_version += 1


def showwarning(message, category, filename, lineno, file=None, line=None):
    """Hook to write a warning to a file; replace if you like."""
    msg = WarningMessage(message, category, filename, lineno, file, line)
    _showwarnmsg_impl(msg)


def formatwarning(message, category, filename, lineno, line=None):
    """Function to format a warning the standard way."""
    msg = WarningMessage(message, category, filename, lineno, None, line)
    return _formatwarnmsg_impl(msg)


def _showwarnmsg_impl(msg):
    file = msg.file
    if file is None:
        file = sys.stderr
        if file is None:
            return
    text = _formatwarnmsg(msg)
    try:
        file.write(text)
    except OSError:
        pass


def _formatwarnmsg_impl(msg):
    category = msg.category.__name__
    s = f"{msg.filename}:{msg.lineno}: {category}: {msg.message}\n"
    line = msg.line
    if line:
        line = line.strip()
        s += "  %s\n" % line
    return s


_showwarning_orig = showwarning
_formatwarning_orig = formatwarning


def _showwarnmsg(msg):
    sw = showwarning
    if sw is not _showwarning_orig:
        if not callable(sw):
            raise TypeError("warnings.showwarning() must be set to a "
                            "function or method")
        sw(msg.message, msg.category, msg.filename, msg.lineno,
           msg.file, msg.line)
        return
    _showwarnmsg_impl(msg)


def _formatwarnmsg(msg):
    fw = formatwarning
    if fw is not _formatwarning_orig:
        return fw(msg.message, msg.category,
                  msg.filename, msg.lineno, msg.line)
    return _formatwarnmsg_impl(msg)


def filterwarnings(action, message="", category=Warning, module="", lineno=0,
                   append=False):
    """Insert an entry into the list of warnings filters (at the front)."""
    if action not in _ACTIONS:
        raise ValueError(f"invalid action: {action!r}")
    if not isinstance(message, str):
        raise TypeError("message must be a string")
    if not isinstance(category, type) or not issubclass(category, Warning):
        raise TypeError("category must be a Warning subclass")
    if not isinstance(module, str):
        raise TypeError("module must be a string")
    if not isinstance(lineno, int):
        raise TypeError("lineno must be an int")
    if lineno < 0:
        raise ValueError("lineno must be an int >= 0")
    if message or module:
        import re
    if message:
        message = re.compile(message, re.I)
    else:
        message = None
    if module:
        module = re.compile(module)
    else:
        module = None
    _add_filter(action, message, category, module, lineno, append=append)


def simplefilter(action, category=Warning, lineno=0, append=False):
    """Insert a simple entry into the list of warnings filters (at the front)."""
    if action not in _ACTIONS:
        raise ValueError(f"invalid action: {action!r}")
    if not isinstance(lineno, int):
        raise TypeError("lineno must be an int")
    if lineno < 0:
        raise ValueError("lineno must be an int >= 0")
    _add_filter(action, None, category, None, lineno, append=append)


def _add_filter(*item, append):
    if not append:
        try:
            filters.remove(item)
        except ValueError:
            pass
        filters.insert(0, item)
    elif item not in filters:
        filters.append(item)
    _filters_mutated()


def resetwarnings():
    """Clear the list of warning filters, so that no filters are active."""
    del filters[:]
    _filters_mutated()


def warn(message, category=None, stacklevel=1, source=None,
         *, skip_file_prefixes=()):
    """Issue a warning, or maybe ignore it or raise an exception."""
    if isinstance(message, Warning):
        category = message.__class__
    if category is None:
        category = UserWarning
    if not (isinstance(category, type) and issubclass(category, Warning)):
        raise TypeError("category must be a Warning subclass, "
                        "not '{:s}'".format(type(category).__name__))
    if not isinstance(skip_file_prefixes, tuple):
        raise TypeError("warn() argument 'skip_file_prefixes' must be tuple, "
                        f"not {type(skip_file_prefixes).__name__}")
    if skip_file_prefixes and stacklevel < 2:
        stacklevel = 2
    frame = _caller(stacklevel, skip_file_prefixes)
    if frame is None:
        filename, lineno, module, registry = "<sys>", 0, "sys", _registry
    else:
        filename, lineno, globals = frame
        module = globals.get("__name__", "<string>")
        registry = globals.setdefault("__warningregistry__", {})
    warn_explicit(message, category, filename, lineno, module, registry,
                  source=source)


def _match(pattern, s):
    if pattern is None:
        return True
    if isinstance(pattern, str):
        return pattern == s
    return pattern.match(s)


def warn_explicit(message, category, filename, lineno,
                  module=None, registry=None, module_globals=None,
                  source=None):
    lineno = int(lineno)
    if module is None:
        module = filename or "<unknown>"
        if module[-3:].lower() == ".py":
            module = module[:-3]
    if isinstance(message, Warning):
        text = str(message)
        category = message.__class__
    else:
        text = message
        message = category(message)
    key = (text, category, lineno)
    if registry is None:
        registry = {}
    if registry.get('version', 0) != _filters_version:
        registry.clear()
        registry['version'] = _filters_version
    if registry.get(key):
        return
    item = None
    for item in filters:
        action, msg, cat, mod, ln = item
        if (_match(msg, text) and issubclass(category, cat) and
                _match(mod, module) and (ln == 0 or lineno == ln)):
            break
    else:
        action = defaultaction
    if action == "ignore":
        return
    if action == "error":
        raise message
    if action == "once":
        registry[key] = 1
        oncekey = (text, category)
        if onceregistry.get(oncekey):
            return
        onceregistry[oncekey] = 1
    elif action in ("always", "all"):
        pass
    elif action == "module":
        registry[key] = 1
        altkey = (text, category, 0)
        if registry.get(altkey):
            return
        registry[altkey] = 1
    elif action == "default":
        registry[key] = 1
    else:
        raise RuntimeError(
              "Unrecognized action (%r) in warnings.filters:\n %s" %
              (action, item))
    _showwarnmsg(WarningMessage(message, category, filename, lineno,
                                source=source))


class WarningMessage(object):

    _WARNING_DETAILS = ("message", "category", "filename", "lineno", "file",
                        "line", "source")

    def __init__(self, message, category, filename, lineno, file=None,
                 line=None, source=None):
        self.message = message
        self.category = category
        self.filename = filename
        self.lineno = lineno
        self.file = file
        self.line = line
        self.source = source
        self._category_name = category.__name__ if category else None

    def __str__(self):
        return ("{message : %r, category : %r, filename : %r, lineno : %s, "
                "line : %r}" % (self.message, self._category_name,
                                self.filename, self.lineno, self.line))

    def __repr__(self):
        return f'<{type(self).__qualname__} {self}>'


class catch_warnings(object):
    """A context manager that copies and restores the warnings filters,
    showwarning() and the recording hook on exit. With record=True it
    returns the list the warnings are appended to (as WarningMessage)."""

    def __init__(self, *, record=False, module=None,
                 action=None, category=Warning, lineno=0, append=False):
        self._record = record
        self._entered = False
        if action is None:
            self._filter = None
        else:
            self._filter = (action, category, lineno, append)

    def __repr__(self):
        args = []
        if self._record:
            args.append("record=True")
        name = type(self).__name__
        return "%s(%s)" % (name, ", ".join(args))

    def __enter__(self):
        global filters, showwarning, _showwarnmsg_impl
        if self._entered:
            raise RuntimeError("Cannot enter %r twice" % self)
        self._entered = True
        self._filters = filters
        filters = filters[:]
        self._showwarnmsg_impl = _showwarnmsg_impl
        self._showwarning = showwarning
        _filters_mutated()
        if self._record:
            log = []
            _showwarnmsg_impl = log.append
            showwarning = _showwarning_orig
        else:
            log = None
        if self._filter is not None:
            simplefilter(*self._filter)
        return log

    def __exit__(self, *exc_info):
        global filters, showwarning, _showwarnmsg_impl
        if not self._entered:
            raise RuntimeError("Cannot exit %r without entering first" % self)
        filters = self._filters
        _showwarnmsg_impl = self._showwarnmsg_impl
        showwarning = self._showwarning
        _filters_mutated()


class deprecated:
    """Indicate that a class or function is deprecated (PEP 702): calling
    the function, instantiating the class or subclassing it issues
    *category* (nothing when it is None)."""

    def __init__(self, message, /, *, category=DeprecationWarning,
                 stacklevel=1):
        if not isinstance(message, str):
            raise TypeError(
                f"Expected an object of type str for 'message', not {type(message).__name__!r}"
            )
        self.message = message
        self.category = category
        self.stacklevel = stacklevel

    def __call__(self, arg, /):
        msg = self.message
        category = self.category
        stacklevel = self.stacklevel
        if category is None:
            arg.__deprecated__ = msg
            return arg
        elif isinstance(arg, type):
            original_init = arg.__init__

            def __init__(self, *args, **kwargs):
                if type(self) is arg:
                    warn(msg, category=category, stacklevel=stacklevel + 1)
                return original_init(self, *args, **kwargs)

            if "__init_subclass__" in arg.__dict__:
                original_init_subclass = arg.__init_subclass__
                if hasattr(original_init_subclass, "__func__"):
                    original_init_subclass = original_init_subclass.__func__

                def __init_subclass__(*args, **kwargs):
                    warn(msg, category=category, stacklevel=stacklevel + 1)
                    return original_init_subclass(*args, **kwargs)
            else:
                def __init_subclass__(cls, *args, **kwargs):
                    warn(msg, category=category, stacklevel=stacklevel + 1)
                    return super(arg, cls).__init_subclass__(*args, **kwargs)

            arg.__init__ = __init__
            arg.__init_subclass__ = classmethod(__init_subclass__)
            arg.__deprecated__ = msg
            return arg
        elif callable(arg):
            import functools

            @functools.wraps(arg)
            def wrapper(*args, **kwargs):
                warn(msg, category=category, stacklevel=stacklevel + 1)
                return arg(*args, **kwargs)

            arg.__deprecated__ = wrapper.__deprecated__ = msg
            return wrapper
        else:
            raise TypeError(
                "@deprecated decorator with non-None category must be applied to "
                f"a class or callable, not {arg!r}"
            )


# CPython's defaults, as its C _warnings module stores them.
filters.append(("default", None, DeprecationWarning, "__main__", 0))
simplefilter("ignore", category=DeprecationWarning, append=1)
simplefilter("ignore", category=PendingDeprecationWarning, append=1)
simplefilter("ignore", category=ImportWarning, append=1)
simplefilter("ignore", category=ResourceWarning, append=1)
