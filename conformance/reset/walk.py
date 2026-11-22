# The reset walk (README.md here): everything a cell can reach from the names
# every cell starts with (the builtins, the host's definitions and every
# importable module), by attribute and by item.
#
# `MODE = 'mark'` writes a mark to each object reached, every way a cell can
# write: an item, an attribute, a function's defaults, name and doc.
# `MODE = 'check'` reports the marks it finds. A cell that marks, a
# `reset()`, and a cell that checks must print what the check prints in a
# new session: no mark, and as many objects. In one session without the
# reset, the check finds the marks (the test's control).
#
# The names are CPython 3.14's `dir(builtins)` and `sys.stdlib_module_names`
# with mm-stdlib's submodules and the test's host definitions; a name or
# module this build does not have is skipped.

MARK = '_mm_reset_mark'
LIMIT = 200000
DEPTH = 6

ROOTS = [
    ('ArithmeticError', lambda: ArithmeticError), ('AssertionError', lambda: AssertionError),
    ('AttributeError', lambda: AttributeError), ('BaseException', lambda: BaseException),
    ('BaseExceptionGroup', lambda: BaseExceptionGroup), ('BlockingIOError', lambda: BlockingIOError),
    ('BrokenPipeError', lambda: BrokenPipeError), ('BufferError', lambda: BufferError), ('BytesWarning',
    lambda: BytesWarning), ('ChildProcessError', lambda: ChildProcessError), ('ConnectionAbortedError',
    lambda: ConnectionAbortedError), ('ConnectionError', lambda: ConnectionError),
    ('ConnectionRefusedError', lambda: ConnectionRefusedError), ('ConnectionResetError', lambda:
    ConnectionResetError), ('DeprecationWarning', lambda: DeprecationWarning), ('EOFError', lambda:
    EOFError), ('Ellipsis', lambda: Ellipsis), ('EncodingWarning', lambda: EncodingWarning),
    ('EnvironmentError', lambda: EnvironmentError), ('Exception', lambda: Exception), ('ExceptionGroup',
    lambda: ExceptionGroup), ('False', lambda: False), ('FileExistsError', lambda: FileExistsError),
    ('FileNotFoundError', lambda: FileNotFoundError), ('FloatingPointError', lambda:
    FloatingPointError), ('FutureWarning', lambda: FutureWarning), ('GeneratorExit', lambda:
    GeneratorExit), ('IOError', lambda: IOError), ('ImportError', lambda: ImportError),
    ('ImportWarning', lambda: ImportWarning), ('IndentationError', lambda: IndentationError),
    ('IndexError', lambda: IndexError), ('InterruptedError', lambda: InterruptedError),
    ('IsADirectoryError', lambda: IsADirectoryError), ('KeyError', lambda: KeyError),
    ('KeyboardInterrupt', lambda: KeyboardInterrupt), ('LookupError', lambda: LookupError),
    ('MemoryError', lambda: MemoryError), ('ModuleNotFoundError', lambda: ModuleNotFoundError),
    ('NameError', lambda: NameError), ('None', lambda: None), ('NotADirectoryError', lambda:
    NotADirectoryError), ('NotImplemented', lambda: NotImplemented), ('NotImplementedError', lambda:
    NotImplementedError), ('OSError', lambda: OSError), ('OverflowError', lambda: OverflowError),
    ('PendingDeprecationWarning', lambda: PendingDeprecationWarning), ('PermissionError', lambda:
    PermissionError), ('ProcessLookupError', lambda: ProcessLookupError), ('PythonFinalizationError',
    lambda: PythonFinalizationError), ('RecursionError', lambda: RecursionError), ('ReferenceError',
    lambda: ReferenceError), ('ResourceWarning', lambda: ResourceWarning), ('RuntimeError', lambda:
    RuntimeError), ('RuntimeWarning', lambda: RuntimeWarning), ('StopAsyncIteration', lambda:
    StopAsyncIteration), ('StopIteration', lambda: StopIteration), ('SyntaxError', lambda: SyntaxError),
    ('SyntaxWarning', lambda: SyntaxWarning), ('SystemError', lambda: SystemError), ('SystemExit',
    lambda: SystemExit), ('TabError', lambda: TabError), ('TimeoutError', lambda: TimeoutError),
    ('True', lambda: True), ('TypeError', lambda: TypeError), ('UnboundLocalError', lambda:
    UnboundLocalError), ('UnicodeDecodeError', lambda: UnicodeDecodeError), ('UnicodeEncodeError',
    lambda: UnicodeEncodeError), ('UnicodeError', lambda: UnicodeError), ('UnicodeTranslateError',
    lambda: UnicodeTranslateError), ('UnicodeWarning', lambda: UnicodeWarning), ('UserWarning', lambda:
    UserWarning), ('ValueError', lambda: ValueError), ('Warning', lambda: Warning),
    ('ZeroDivisionError', lambda: ZeroDivisionError), ('__import__', lambda: __import__), ('abs',
    lambda: abs), ('aiter', lambda: aiter), ('all', lambda: all), ('anext', lambda: anext), ('any',
    lambda: any), ('ascii', lambda: ascii), ('bin', lambda: bin), ('bool', lambda: bool), ('breakpoint',
    lambda: breakpoint), ('bytearray', lambda: bytearray), ('bytes', lambda: bytes), ('callable',
    lambda: callable), ('chr', lambda: chr), ('classmethod', lambda: classmethod), ('compile', lambda:
    compile), ('complex', lambda: complex), ('copyright', lambda: copyright), ('credits', lambda:
    credits), ('delattr', lambda: delattr), ('dict', lambda: dict), ('dir', lambda: dir), ('divmod',
    lambda: divmod), ('enumerate', lambda: enumerate), ('eval', lambda: eval), ('exec', lambda: exec),
    ('exit', lambda: exit), ('filter', lambda: filter), ('float', lambda: float), ('format', lambda:
    format), ('frozenset', lambda: frozenset), ('getattr', lambda: getattr), ('globals', lambda:
    globals), ('hasattr', lambda: hasattr), ('hash', lambda: hash), ('help', lambda: help), ('hex',
    lambda: hex), ('id', lambda: id), ('input', lambda: input), ('int', lambda: int), ('isinstance',
    lambda: isinstance), ('issubclass', lambda: issubclass), ('iter', lambda: iter), ('len', lambda:
    len), ('license', lambda: license), ('list', lambda: list), ('locals', lambda: locals), ('map',
    lambda: map), ('max', lambda: max), ('memoryview', lambda: memoryview), ('min', lambda: min),
    ('next', lambda: next), ('object', lambda: object), ('oct', lambda: oct), ('open', lambda: open),
    ('ord', lambda: ord), ('pow', lambda: pow), ('print', lambda: print), ('property', lambda:
    property), ('quit', lambda: quit), ('range', lambda: range), ('repr', lambda: repr), ('reversed',
    lambda: reversed), ('round', lambda: round), ('set', lambda: set), ('setattr', lambda: setattr),
    ('slice', lambda: slice), ('sorted', lambda: sorted), ('staticmethod', lambda: staticmethod),
    ('str', lambda: str), ('sum', lambda: sum), ('super', lambda: super), ('tuple', lambda: tuple),
    ('type', lambda: type), ('vars', lambda: vars), ('zip', lambda: zip), ('h', lambda: h), ('tool',
    lambda: tool), ('rec', lambda: rec), ('obj', lambda: obj), ('boom', lambda: boom), ('Entry', lambda:
    Entry), ('ToolError', lambda: ToolError), ('CONF', lambda: CONF), ('CONFD', lambda: CONFD),
    ('CONFR', lambda: CONFR),
]

MODULES = (
    '_markupbase', '_mm', 'abc', 'annotationlib', 'argparse', 'array', 'ast', 'asyncio',
    'asyncio.events', 'asyncio.futures', 'asyncio.locks', 'asyncio.queues', 'asyncio.runners',
    'asyncio.taskgroups', 'asyncio.tasks', 'asyncio.timeouts', 'atexit', 'base64', 'bdb', 'binascii',
    'bisect', 'builtins', 'bz2', 'cProfile', 'calendar', 'cmath', 'cmd', 'code', 'codecs', 'codeop',
    'collections', 'collections.abc', 'colorsys', 'compileall', 'compression', 'concurrent',
    'configparser', 'contextlib', 'contextvars', 'copy', 'copyreg', 'csv', 'ctypes', 'curses',
    'dataclasses', 'datetime', 'datetime._strptime', 'dbm', 'decimal', 'difflib', 'dis', 'doctest',
    'email', 'email.utils', 'encodings', 'ensurepip', 'enum', 'errno', 'faulthandler', 'fcntl',
    'filecmp', 'fileinput', 'fnmatch', 'fractions', 'fs', 'ftplib', 'functools', 'gc', 'genericpath',
    'getopt', 'getpass', 'gettext', 'glob', 'graphlib', 'grp', 'gzip', 'hashlib', 'heapq', 'hmac',
    'html', 'html.entities', 'html.parser', 'http', 'imaplib', 'importlib', 'inspect', 'io',
    'ipaddress', 'itertools', 'json', 'json.decoder', 'json.encoder', 'keyword', 'linecache', 'locale',
    'logging', 'lzma', 'mailbox', 'marshal', 'math', 'mimetypes', 'mmap', 'modulefinder', 'msvcrt',
    'multiprocessing', 'netrc', 'nt', 'ntpath', 'nturl2path', 'numbers', 'opcode', 'operator',
    'optparse', 'os', 'os.path', 'pathlib', 'pdb', 'pickle', 'pickletools', 'pkgutil', 'platform',
    'plistlib', 'poplib', 'posix', 'posixpath', 'pprint', 'profile', 'pstats', 'pty', 'pwd',
    'py_compile', 'pyclbr', 'pydoc', 'pydoc_data', 'pyexpat', 'queue', 'quopri', 'random', 're',
    'readline', 'reprlib', 'resource', 'rlcompleter', 'runpy', 'sched', 'secrets', 'select',
    'selectors', 'shelve', 'shlex', 'shutil', 'signal', 'site', 'smtplib', 'socket', 'socketserver',
    'sqlite3', 'sre_compile', 'sre_constants', 'sre_parse', 'ssl', 'stat', 'statistics', 'string',
    'stringprep', 'struct', 'subprocess', 'symtable', 'sys', 'sysconfig', 'syslog', 'tabnanny',
    'tarfile', 'tempfile', 'termios', 'textwrap', 'threading', 'time', 'timeit', 'token', 'tokenize',
    'tomllib', 'tomllib._parser', 'tomllib._re', 'tomllib._types', 'trace', 'traceback', 'tracemalloc',
    'tty', 'types', 'typing', 'unicodedata', 'unittest', 'urllib', 'urllib.parse', 'uuid', 'venv',
    'warnings', 'wave', 'weakref', 'webbrowser', 'winreg', 'winsound', 'wsgiref', 'xml', 'xml.etree',
    'xml.etree.ElementPath', 'xml.etree.ElementTree', 'xmlrpc', 'zipapp', 'zipfile', 'zipimport',
    'zlib', 'zoneinfo', 'zoneinfo._common', 'zoneinfo._tzpath', 'zoneinfo._zoneinfo',
)

# Looked up beside `dir()`'s names: what `dir()` may leave out.
EXTRA = (
    '__annotations__', '__args__', '__bases__', '__builtins__', '__cause__', '__class__',
    '__closure__', '__code__', '__context__', '__defaults__', '__dict__', '__func__',
    '__globals__', '__kwdefaults__', '__match_args__', '__mro__', '__notes__', '__objclass__',
    '__origin__', '__self__', '__slots__', '__wrapped__', '_fields', 'fdel', 'fget', 'fset',
)

# Values with nothing a cell can write and nothing below them.
LEAVES = (
    'str', 'bytes', 'int', 'float', 'bool', 'complex', 'NoneType', 'range', 'slice', 'ellipsis',
    'NotImplementedType', 'builtin_function_or_method', 'method', 'method-wrapper',
    'wrapper_descriptor', 'method_descriptor', 'classmethod_descriptor', 'getset_descriptor',
    'member_descriptor',
)


def roots():
    out = []
    for name, get in ROOTS:
        try:
            out.append((name, get()))
        except NameError:
            pass
    for name in MODULES:
        try:
            m = __import__(name)
        except Exception:
            continue
        for part in name.split('.')[1:]:
            m = getattr(m, part, None)
        if m is not None:
            out.append((name, m))
    # What the host's functions return and raise.
    for name in ('obj', 'rec', 'boom'):
        try:
            out.append((name + '()', dict(ROOTS)[name]()()))
        except NameError:
            pass
        except Exception as e:
            out.append((name + '() raised', e))
    return out


def children(o):
    t = type(o)
    if t is list or t is tuple or t is set or t is frozenset:
        i = 0
        for v in list(o):
            yield '[' + str(i) + ']', v
            i += 1
        return
    if t is dict:
        i = 0
        for k, v in list(o.items()):
            yield '.key' + str(i), k
            yield '[' + repr(k)[:40] + ']', v
            i += 1
        return
    try:
        names = list(dir(o))
    except Exception:
        names = []
    for n in names + [n for n in EXTRA if n not in names]:
        try:
            v = getattr(o, n)
        except Exception:
            continue
        yield '.' + n, v


def walk():
    seen = {}
    found = []
    todo = [(name, 0, v) for name, v in roots()]
    todo.reverse()
    while todo and len(found) < LIMIT:
        path, depth, o = todo.pop()
        if type(o).__name__ in LEAVES or id(o) in seen:
            continue
        # Held, so no id is used twice.
        seen[id(o)] = o
        found.append((path, o))
        # A property that makes a new object on every read never ends.
        if depth == DEPTH:
            continue
        kids = [(path + p, depth + 1, v) for p, v in children(o)
                if type(v).__name__ not in LEAVES and id(v) not in seen]
        kids.reverse()
        todo.extend(kids)
    return found


def attempt(f, *args):
    try:
        f(*args)
        return 1
    except Exception:
        return 0


def mark(o):
    t = type(o)
    if t is list:
        return attempt(o.append, MARK)
    if t is dict:
        return attempt(o.__setitem__, MARK, MARK)
    if t is set:
        return attempt(o.add, MARK)
    if t is bytearray:
        return attempt(o.extend, MARK.encode())
    if t is tuple or t is frozenset:
        return 0
    n = attempt(setattr, o, MARK, MARK)
    n += attempt(object.__setattr__, o, MARK, MARK)
    if isinstance(o, type):
        n += attempt(type.__setattr__, o, MARK, MARK)
    for a, v in (('__doc__', MARK), ('__name__', MARK), ('__qualname__', MARK),
                 ('__defaults__', (MARK,)), ('__kwdefaults__', {MARK: MARK}),
                 ('__dict__', {MARK: MARK}), ('__annotations__', {MARK: MARK}),
                 ('__wrapped__', MARK), ('__module__', MARK)):
        n += attempt(setattr, o, a, v)
    return n


def marked(o):
    t = type(o)
    try:
        if t is list or t is dict or t is set or t is tuple or t is frozenset:
            return MARK in o
        if t is bytearray:
            return MARK.encode() in o
        if getattr(o, MARK, None) is MARK or getattr(o, MARK, None) == MARK:
            return True
        for a in ('__doc__', '__name__', '__qualname__', '__wrapped__', '__module__'):
            if getattr(o, a, None) == MARK:
                return True
    except Exception:
        pass
    return False


found = walk()
if MODE == 'mark':
    print('marked', sum(1 for _, o in found if mark(o)) > 0)
else:
    hits = sorted(path for path, o in found if marked(o))
    print('reached', len(found), 'limit', len(found) >= LIMIT)
    print('marks', len(hits), hits[:40])
