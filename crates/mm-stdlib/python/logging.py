# logging: a slim CPython 3.14 logging. Kept: levels, LogRecord, the three
# Formatter styles, Filter, Handler, StreamHandler, NullHandler, the logger
# hierarchy, basicConfig and the module-level functions, with CPython's
# behaviour and messages. Left out: logging.handlers and logging.config,
# FileHandler (no filesystem; basicConfig(filename=...) calls open()),
# LoggerAdapter, stack_info, locks, captureWarnings and shutdown. There are
# no frame objects, so records carry CPython's values for an unknown caller
# (pathname and filename "(unknown file)", lineno 0, funcName
# "(unknown function)"), and no process or thread ids.

import sys
import time
import traceback

__all__ = ['BASIC_FORMAT', 'CRITICAL', 'DEBUG', 'ERROR', 'FATAL', 'Filter', 'Filterer',
           'Formatter', 'Handler', 'INFO', 'LogRecord', 'Logger', 'NOTSET', 'NullHandler',
           'StreamHandler', 'WARN', 'WARNING', 'addLevelName', 'basicConfig', 'critical',
           'debug', 'disable', 'error', 'exception', 'fatal', 'getLevelName', 'getLogger',
           'getLoggerClass', 'info', 'lastResort', 'log', 'makeLogRecord', 'root',
           'setLoggerClass', 'warn', 'warning', 'getLevelNamesMapping', 'getHandlerByName',
           'getHandlerNames', 'PercentStyle', 'StrFormatStyle', 'StringTemplateStyle']

_startTime = time.time_ns()

raiseExceptions = True
logThreads = True
logMultiprocessing = True
logProcesses = True
logAsyncioTasks = True

CRITICAL = 50
FATAL = CRITICAL
ERROR = 40
WARNING = 30
WARN = WARNING
INFO = 20
DEBUG = 10
NOTSET = 0

_levelToName = {
    CRITICAL: 'CRITICAL',
    ERROR: 'ERROR',
    WARNING: 'WARNING',
    INFO: 'INFO',
    DEBUG: 'DEBUG',
    NOTSET: 'NOTSET',
}
_nameToLevel = {
    'CRITICAL': CRITICAL,
    'FATAL': FATAL,
    'ERROR': ERROR,
    'WARN': WARNING,
    'WARNING': WARNING,
    'INFO': INFO,
    'DEBUG': DEBUG,
    'NOTSET': NOTSET,
}


def getLevelNamesMapping():
    return _nameToLevel.copy()


def getLevelName(level):
    """Return the textual or numeric representation of logging level 'level'."""
    result = _levelToName.get(level)
    if result is not None:
        return result
    result = _nameToLevel.get(level)
    if result is not None:
        return result
    return "Level %s" % level


def addLevelName(level, levelName):
    """Associate 'levelName' with 'level'."""
    _levelToName[level] = levelName
    _nameToLevel[levelName] = level


def _checkLevel(level):
    if isinstance(level, int):
        rv = level
    elif str(level) == level:
        if level not in _nameToLevel:
            raise ValueError("Unknown level: %r" % level)
        rv = _nameToLevel[level]
    else:
        raise TypeError("Level not an integer or a valid string: %r" % (level,))
    return rv


class LogRecord(object):
    """A LogRecord instance represents an event being logged."""

    def __init__(self, name, level, pathname, lineno, msg, args, exc_info, func=None,
                 sinfo=None, **kwargs):
        ct = time.time_ns()
        self.name = name
        self.msg = msg
        if (args and len(args) == 1 and isinstance(args[0], dict) and args[0]):
            args = args[0]
        self.args = args
        self.levelname = getLevelName(level)
        self.levelno = level
        self.pathname = pathname
        try:
            self.filename = pathname.rsplit('/', 1)[-1]
            self.module = self.filename.rsplit('.', 1)[0] if '.' in self.filename[1:] \
                else self.filename
        except (TypeError, ValueError, AttributeError):
            self.filename = pathname
            self.module = "Unknown module"
        self.exc_info = exc_info
        self.exc_text = None
        self.stack_info = sinfo
        self.lineno = lineno
        self.funcName = func
        self.created = ct / 1e9
        self.msecs = (ct % 1_000_000_000) // 1_000_000 + 0.0
        self.relativeCreated = (ct - _startTime) / 1e6
        self.thread = None
        self.threadName = None
        self.processName = None
        self.process = None
        self.taskName = None

    def __repr__(self):
        return '<LogRecord: %s, %s, %s, %s, "%s">' % (self.name, self.levelno,
                                                      self.pathname, self.lineno, self.msg)

    def getMessage(self):
        """Return the message for this LogRecord."""
        msg = str(self.msg)
        if self.args:
            msg = msg % self.args
        return msg


_logRecordFactory = LogRecord


def setLogRecordFactory(factory):
    global _logRecordFactory
    _logRecordFactory = factory


def getLogRecordFactory():
    return _logRecordFactory


def makeLogRecord(dict):
    """Make a LogRecord whose attributes are defined by the specified dictionary."""
    rv = _logRecordFactory(None, None, "", 0, "", (), None, None)
    rv.__dict__.update(dict)
    return rv


class PercentStyle(object):

    default_format = '%(message)s'
    asctime_format = '%(asctime)s'
    asctime_search = '%(asctime)'

    def __init__(self, fmt, *, defaults=None):
        self._fmt = fmt or self.default_format
        self._defaults = defaults

    def usesTime(self):
        return self._fmt.find(self.asctime_search) >= 0

    def validate(self):
        """Validate the input format, ensure it matches the correct style"""
        import re
        if not re.search(r'%\(\w+\)[#0+ -]*(\*|\d+)?(\.(\*|\d+))?[diouxefgcrsa%]',
                         self._fmt, re.I):
            raise ValueError("Invalid format '%s' for '%s' style" % (self._fmt,
                                                                      self.default_format[0]))

    def _format(self, record):
        if defaults := self._defaults:
            values = defaults | record.__dict__
        else:
            values = record.__dict__
        return self._fmt % values

    def format(self, record):
        try:
            return self._format(record)
        except KeyError as e:
            raise ValueError('Formatting field not found in record: %s' % e)


class StrFormatStyle(PercentStyle):
    default_format = '{message}'
    asctime_format = '{asctime}'
    asctime_search = '{asctime'

    def _format(self, record):
        if defaults := self._defaults:
            values = defaults | record.__dict__
        else:
            values = record.__dict__
        return self._fmt.format(**values)

    def validate(self):
        """Validate the input format, ensure it is the correct string formatting style"""
        import re
        if not re.search(r'\{[^{}]+\}', self._fmt):
            raise ValueError('invalid format: no fields')


class StringTemplateStyle(PercentStyle):
    default_format = '${message}'
    asctime_format = '${asctime}'
    asctime_search = '${asctime}'

    def __init__(self, *args, **kwargs):
        from string import Template
        super().__init__(*args, **kwargs)
        self._tpl = Template(self._fmt)

    def usesTime(self):
        fmt = self._fmt
        return fmt.find('$asctime') >= 0 or fmt.find(self.asctime_search) >= 0

    def validate(self):
        import re
        if not re.search(r'\$(\w+|\{\w+\})', self._fmt):
            raise ValueError('invalid format: no fields')

    def _format(self, record):
        if defaults := self._defaults:
            values = defaults | record.__dict__
        else:
            values = record.__dict__
        return self._tpl.substitute(**values)


BASIC_FORMAT = "%(levelname)s:%(name)s:%(message)s"

_STYLES = {
    '%': (PercentStyle, BASIC_FORMAT),
    '{': (StrFormatStyle, '{levelname}:{name}:{message}'),
    '$': (StringTemplateStyle, '${levelname}:${name}:${message}'),
}


class Formatter(object):
    """Formatter instances are used to convert a LogRecord to text."""

    converter = time.localtime

    def __init__(self, fmt=None, datefmt=None, style='%', validate=True, *, defaults=None):
        if style not in _STYLES:
            raise ValueError('Style must be one of: %s' % ','.join(_STYLES.keys()))
        self._style = _STYLES[style][0](fmt, defaults=defaults)
        if validate:
            self._style.validate()
        self._fmt = self._style._fmt
        self.datefmt = datefmt

    default_time_format = '%Y-%m-%d %H:%M:%S'
    default_msec_format = '%s,%03d'

    def formatTime(self, record, datefmt=None):
        """Return the creation time of the specified LogRecord as formatted text."""
        conv = self.converter
        # micro-monty's time functions are Python functions, which bind
        # as methods here; CPython's are builtins.
        if getattr(conv, '__self__', None) is self:
            conv = conv.__func__
        ct = conv(record.created)
        if datefmt:
            s = time.strftime(datefmt, ct)
        else:
            s = time.strftime(self.default_time_format, ct)
            if self.default_msec_format:
                s = self.default_msec_format % (s, record.msecs)
        return s

    def formatException(self, ei):
        """Format and return the specified exception information as a string."""
        s = ''.join(traceback.format_exception(ei[0], ei[1], ei[2]))
        if s[-1:] == "\n":
            s = s[:-1]
        return s

    def usesTime(self):
        """Check if the format uses the creation time of the record."""
        return self._style.usesTime()

    def formatMessage(self, record):
        return self._style.format(record)

    def formatStack(self, stack_info):
        return stack_info

    def format(self, record):
        """Format the specified record as text."""
        record.message = record.getMessage()
        if self.usesTime():
            record.asctime = self.formatTime(record, self.datefmt)
        s = self.formatMessage(record)
        if record.exc_info:
            if not record.exc_text:
                record.exc_text = self.formatException(record.exc_info)
        if record.exc_text:
            if s[-1:] != "\n":
                s = s + "\n"
            s = s + record.exc_text
        if record.stack_info:
            if s[-1:] != "\n":
                s = s + "\n"
            s = s + self.formatStack(record.stack_info)
        return s


_defaultFormatter = Formatter()


class Filter(object):
    """Filter instances are used to perform arbitrary filtering of LogRecords."""

    def __init__(self, name=''):
        self.name = name
        self.nlen = len(name)

    def filter(self, record):
        """Determine if the specified record is to be logged."""
        if self.nlen == 0:
            return True
        elif self.name == record.name:
            return True
        elif record.name.find(self.name, 0, self.nlen) != 0:
            return False
        return (record.name[self.nlen] == ".")


class Filterer(object):
    """A base class for loggers and handlers which allows them to share
    common code."""

    def __init__(self):
        self.filters = []

    def addFilter(self, filter):
        if not (filter in self.filters):
            self.filters.append(filter)

    def removeFilter(self, filter):
        if filter in self.filters:
            self.filters.remove(filter)

    def filter(self, record):
        """Determine if a record is loggable by consulting all the filters."""
        for f in self.filters:
            if hasattr(f, 'filter'):
                result = f.filter(record)
            else:
                result = f(record)
            if not result:
                return False
            if isinstance(result, LogRecord):
                record = result
        return record


_handlers = {}
_handlerList = []


class Handler(Filterer):
    """Handler instances dispatch logging events to specific destinations."""

    def __init__(self, level=NOTSET):
        Filterer.__init__(self)
        self._name = None
        self.level = _checkLevel(level)
        self.formatter = None
        self._closed = False
        _handlerList.append(self)

    def get_name(self):
        return self._name

    def set_name(self, name):
        if self._name in _handlers:
            del _handlers[self._name]
        self._name = name
        if name:
            _handlers[name] = self

    name = property(get_name, set_name)

    def createLock(self):
        self.lock = None

    def acquire(self):
        pass

    def release(self):
        pass

    def setLevel(self, level):
        """Set the logging level of this handler."""
        self.level = _checkLevel(level)

    def format(self, record):
        """Format the specified record."""
        if self.formatter:
            fmt = self.formatter
        else:
            fmt = _defaultFormatter
        return fmt.format(record)

    def emit(self, record):
        """Do whatever it takes to actually log the specified logging record."""
        raise NotImplementedError('emit must be implemented '
                                  'by Handler subclasses')

    def handle(self, record):
        """Conditionally emit the specified logging record."""
        rv = self.filter(record)
        if isinstance(rv, LogRecord):
            record = rv
        if rv:
            self.emit(record)
        return rv

    def setFormatter(self, fmt):
        """Set the formatter for this handler."""
        self.formatter = fmt

    def flush(self):
        """Ensure all logging output has been flushed."""
        pass

    def close(self):
        """Tidy up any resources used by the handler."""
        self._closed = True
        if self._name and self._name in _handlers:
            del _handlers[self._name]

    def handleError(self, record):
        """Handle errors which occur during an emit() call."""
        if raiseExceptions and sys.stderr:
            t, v, tb = sys.exc_info()
            try:
                sys.stderr.write('--- Logging error ---\n')
                traceback.print_exception(t, v, tb, None, sys.stderr)
                try:
                    sys.stderr.write('Message: %r\n'
                                     'Arguments: %s\n' % (record.msg, record.args))
                except RecursionError:
                    raise
                except Exception:
                    sys.stderr.write('Unable to print the message and arguments'
                                     ' - possible formatting error.\nUse the'
                                     ' traceback above to help find the error.\n')
            except OSError:
                pass

    def __repr__(self):
        level = getLevelName(self.level)
        return '<%s (%s)>' % (self.__class__.__name__, level)


class StreamHandler(Handler):
    """A handler class which writes logging records, appropriately formatted,
    to a stream."""

    terminator = '\n'

    def __init__(self, stream=None):
        Handler.__init__(self)
        if stream is None:
            stream = sys.stderr
        self.stream = stream

    def flush(self):
        """Flushes the stream."""
        if self.stream and hasattr(self.stream, "flush"):
            self.stream.flush()

    def emit(self, record):
        """Emit a record."""
        try:
            msg = self.format(record)
            stream = self.stream
            stream.write(msg + self.terminator)
            self.flush()
        except RecursionError:
            raise
        except Exception:
            self.handleError(record)

    def setStream(self, stream):
        """Sets the StreamHandler's stream to the specified value, if it is
        different."""
        if stream is self.stream:
            result = None
        else:
            result = self.stream
            self.flush()
            self.stream = stream
        return result

    def __repr__(self):
        level = getLevelName(self.level)
        name = getattr(self.stream, 'name', '')
        name = str(name)
        if name:
            name += ' '
        return '<%s %s(%s)>' % (self.__class__.__name__, name, level)


class _StderrHandler(StreamHandler):
    """This class is like a StreamHandler using sys.stderr, but always uses
    whatever sys.stderr is currently set to rather than the value of
    sys.stderr at handler construction time."""

    def __init__(self, level=NOTSET):
        Handler.__init__(self, level)

    @property
    def stream(self):
        return sys.stderr


_defaultLastResort = _StderrHandler(WARNING)
lastResort = _defaultLastResort


def getHandlerByName(name):
    return _handlers.get(name)


def getHandlerNames():
    return frozenset(_handlers)


class Manager(object):
    """There is [under normal circumstances] just one Manager instance, which
    holds the hierarchy of loggers."""

    def __init__(self, rootnode):
        self.root = rootnode
        self.disable = 0
        self.emittedNoHandlerWarning = False
        self.loggerDict = {}
        self.loggerClass = None
        self.logRecordFactory = None

    @property
    def disable(self):
        return self._disable

    @disable.setter
    def disable(self, value):
        self._disable = _checkLevel(value)

    def getLogger(self, name):
        """Get a logger with the specified name (channel name), creating it if
        it doesn't yet exist."""
        if not isinstance(name, str):
            raise TypeError('A logger name must be a string')
        rv = self.loggerDict.get(name)
        if rv is None:
            rv = (self.loggerClass or _loggerClass)(name)
            rv.manager = self
            self.loggerDict[name] = rv
            self._fixupParents(rv)
            self._fixupChildren(rv)
        return rv

    def setLoggerClass(self, klass):
        if klass != Logger:
            if not issubclass(klass, Logger):
                raise TypeError("logger not derived from logging.Logger: "
                                + klass.__name__)
        self.loggerClass = klass

    def setLogRecordFactory(self, factory):
        self.logRecordFactory = factory

    def _fixupParents(self, alogger):
        name = alogger.name
        i = name.rfind(".")
        rv = None
        while (i > 0) and not rv:
            rv = self.loggerDict.get(name[:i])
            i = name.rfind(".", 0, i - 1)
        if not rv:
            rv = self.root
        alogger.parent = rv

    def _fixupChildren(self, alogger):
        name = alogger.name
        prefix = name + '.'
        for c in self.loggerDict.values():
            if c is not alogger and c.name.startswith(prefix):
                p = c.parent
                if p is self.root or not p.name.startswith(prefix):
                    c.parent = alogger

    def _clear_cache(self):
        pass


class Logger(Filterer):
    """Instances of the Logger class represent a single logging channel."""

    def __init__(self, name, level=NOTSET):
        """Initialize the logger with a name and an optional level."""
        Filterer.__init__(self)
        self.name = name
        self.level = _checkLevel(level)
        self.parent = None
        self.propagate = True
        self.handlers = []
        self.disabled = False

    def setLevel(self, level):
        """Set the logging level of this logger.  level must be an int or a str."""
        self.level = _checkLevel(level)

    def debug(self, msg, *args, **kwargs):
        if self.isEnabledFor(DEBUG):
            self._log(DEBUG, msg, args, **kwargs)

    def info(self, msg, *args, **kwargs):
        if self.isEnabledFor(INFO):
            self._log(INFO, msg, args, **kwargs)

    def warning(self, msg, *args, **kwargs):
        if self.isEnabledFor(WARNING):
            self._log(WARNING, msg, args, **kwargs)

    def warn(self, msg, *args, **kwargs):
        self.warning(msg, *args, **kwargs)

    def error(self, msg, *args, **kwargs):
        if self.isEnabledFor(ERROR):
            self._log(ERROR, msg, args, **kwargs)

    def exception(self, msg, *args, exc_info=True, **kwargs):
        """Convenience method for logging an ERROR with exception information."""
        self.error(msg, *args, exc_info=exc_info, **kwargs)

    def critical(self, msg, *args, **kwargs):
        if self.isEnabledFor(CRITICAL):
            self._log(CRITICAL, msg, args, **kwargs)

    def fatal(self, msg, *args, **kwargs):
        self.critical(msg, *args, **kwargs)

    def log(self, level, msg, *args, **kwargs):
        """Log 'msg % args' with the integer severity 'level'."""
        if not isinstance(level, int):
            if raiseExceptions:
                raise TypeError("level must be an integer")
            else:
                return
        if self.isEnabledFor(level):
            self._log(level, msg, args, **kwargs)

    def findCaller(self, stack_info=False, stacklevel=1):
        """micro-monty has no frame objects: CPython's values for an unknown
        caller."""
        return "(unknown file)", 0, "(unknown function)", None

    def makeRecord(self, name, level, fn, lno, msg, args, exc_info, func=None, extra=None,
                   sinfo=None):
        """A factory method which can be overridden in subclasses to create
        specialized LogRecords."""
        rv = _logRecordFactory(name, level, fn, lno, msg, args, exc_info, func, sinfo)
        if extra is not None:
            for key in extra:
                if (key in ["message", "asctime"]) or (key in rv.__dict__):
                    raise KeyError("Attempt to overwrite %r in LogRecord" % key)
                rv.__dict__[key] = extra[key]
        return rv

    def _log(self, level, msg, args, exc_info=None, extra=None, stack_info=False,
             stacklevel=1):
        fn, lno, func, sinfo = self.findCaller(stack_info, stacklevel)
        if exc_info:
            if isinstance(exc_info, BaseException):
                exc_info = (type(exc_info), exc_info, exc_info.__traceback__)
            elif not isinstance(exc_info, tuple):
                exc_info = sys.exc_info()
        record = self.makeRecord(self.name, level, fn, lno, msg, args, exc_info, func, extra,
                                 sinfo)
        self.handle(record)

    def handle(self, record):
        """Call the handlers for the specified record."""
        if self.disabled:
            return
        maybe_record = self.filter(record)
        if not maybe_record:
            return
        if isinstance(maybe_record, LogRecord):
            record = maybe_record
        self.callHandlers(record)

    def addHandler(self, hdlr):
        """Add the specified handler to this logger."""
        if not (hdlr in self.handlers):
            self.handlers.append(hdlr)

    def removeHandler(self, hdlr):
        """Remove the specified handler from this logger."""
        if hdlr in self.handlers:
            self.handlers.remove(hdlr)

    def hasHandlers(self):
        """See if this logger has any handlers configured."""
        c = self
        rv = False
        while c:
            if c.handlers:
                rv = True
                break
            if not c.propagate:
                break
            else:
                c = c.parent
        return rv

    def callHandlers(self, record):
        """Pass a record to all relevant handlers."""
        c = self
        found = 0
        while c:
            for hdlr in c.handlers:
                found = found + 1
                if record.levelno >= hdlr.level:
                    hdlr.handle(record)
            if not c.propagate:
                c = None
            else:
                c = c.parent
        if (found == 0):
            if lastResort:
                if record.levelno >= lastResort.level:
                    lastResort.handle(record)
            elif raiseExceptions and not self.manager.emittedNoHandlerWarning:
                sys.stderr.write("No handlers could be found for logger"
                                 " \"%s\"\n" % self.name)
                self.manager.emittedNoHandlerWarning = True

    def getEffectiveLevel(self):
        """Get the effective level for this logger."""
        logger = self
        while logger:
            if logger.level:
                return logger.level
            logger = logger.parent
        return NOTSET

    def isEnabledFor(self, level):
        """Is this logger enabled for level 'level'?"""
        if self.disabled:
            return False
        if self.manager.disable >= level:
            return False
        return level >= self.getEffectiveLevel()

    def getChild(self, suffix):
        """Get a logger which is a descendant to this one."""
        if self.root is not self:
            suffix = '.'.join((self.name, suffix))
        return self.manager.getLogger(suffix)

    def getChildren(self):
        def _hierlevel(logger):
            if logger is logger.manager.root:
                return 0
            return 1 + logger.name.count('.')

        d = self.manager.loggerDict
        hierlevel = _hierlevel(self)
        return set(item for item in d.values()
                   if item.parent is self and _hierlevel(item) == 1 + hierlevel)

    def __repr__(self):
        level = getLevelName(self.getEffectiveLevel())
        return '<%s %s (%s)>' % (self.__class__.__name__, self.name, level)


class RootLogger(Logger):
    """A root logger is not that different to any other logger, except that
    it must have a logging level and there is only one instance of it in
    the hierarchy."""

    def __init__(self, level):
        Logger.__init__(self, "root", level)


_loggerClass = Logger

root = RootLogger(WARNING)
Logger.root = root
Logger.manager = Manager(Logger.root)


def setLoggerClass(klass):
    """Set the class to be used when instantiating a logger."""
    if klass != Logger:
        if not issubclass(klass, Logger):
            raise TypeError("logger not derived from logging.Logger: "
                            + klass.__name__)
    global _loggerClass
    _loggerClass = klass


def getLoggerClass():
    """Return the class to be used when instantiating a logger."""
    return _loggerClass


def basicConfig(**kwargs):
    """Do basic configuration for the logging system."""
    force = kwargs.pop('force', False)
    encoding = kwargs.pop('encoding', None)
    errors = kwargs.pop('errors', 'backslashreplace')
    if force:
        for h in root.handlers[:]:
            root.removeHandler(h)
            h.close()
    if len(root.handlers) == 0:
        handlers = kwargs.pop("handlers", None)
        if handlers is None:
            if "stream" in kwargs and "filename" in kwargs:
                raise ValueError("'stream' and 'filename' should not be "
                                 "specified together")
        else:
            if "stream" in kwargs or "filename" in kwargs:
                raise ValueError("'stream' or 'filename' should not be "
                                 "specified together with 'handlers'")
        if handlers is None:
            filename = kwargs.pop("filename", None)
            mode = kwargs.pop("filemode", 'a')
            if filename:
                if 'b' in mode:
                    errors = None
                else:
                    encoding = 'locale' if encoding is None else encoding
                h = StreamHandler(open(filename, mode, encoding=encoding, errors=errors))
            else:
                stream = kwargs.pop("stream", None)
                h = StreamHandler(stream)
            handlers = [h]
        dfs = kwargs.pop("datefmt", None)
        style = kwargs.pop("style", '%')
        if style not in _STYLES:
            raise ValueError('Style must be one of: %s' % ','.join(_STYLES.keys()))
        fs = kwargs.pop("format", _STYLES[style][1])
        fmt = Formatter(fs, dfs, style)
        for h in handlers:
            if h.formatter is None:
                h.setFormatter(fmt)
            root.addHandler(h)
        level = kwargs.pop("level", None)
        if level is not None:
            root.setLevel(level)
        if kwargs:
            keys = ', '.join(kwargs.keys())
            raise ValueError('Unrecognised argument(s): %s' % keys)


def getLogger(name=None):
    """Return a logger with the specified name, creating it if necessary."""
    if not name or isinstance(name, str) and name == root.name:
        return root
    return Logger.manager.getLogger(name)


def critical(msg, *args, **kwargs):
    if len(root.handlers) == 0:
        basicConfig()
    root.critical(msg, *args, **kwargs)


def fatal(msg, *args, **kwargs):
    critical(msg, *args, **kwargs)


def error(msg, *args, **kwargs):
    if len(root.handlers) == 0:
        basicConfig()
    root.error(msg, *args, **kwargs)


def exception(msg, *args, exc_info=True, **kwargs):
    error(msg, *args, exc_info=exc_info, **kwargs)


def warning(msg, *args, **kwargs):
    if len(root.handlers) == 0:
        basicConfig()
    root.warning(msg, *args, **kwargs)


def warn(msg, *args, **kwargs):
    warning(msg, *args, **kwargs)


def info(msg, *args, **kwargs):
    if len(root.handlers) == 0:
        basicConfig()
    root.info(msg, *args, **kwargs)


def debug(msg, *args, **kwargs):
    if len(root.handlers) == 0:
        basicConfig()
    root.debug(msg, *args, **kwargs)


def log(level, msg, *args, **kwargs):
    if len(root.handlers) == 0:
        basicConfig()
    root.log(level, msg, *args, **kwargs)


def disable(level=CRITICAL):
    """Disable all logging calls of severity 'level' and below."""
    root.manager.disable = level
    root.manager._clear_cache()


class NullHandler(Handler):
    """This handler does nothing."""

    def handle(self, record):
        """Stub."""

    def emit(self, record):
        """Stub."""

    def createLock(self):
        self.lock = None
