# The reset sweep (README.md here): every callable in `dir()` of the builtin
# types, called with each object a reset shares as `self`. A native method
# that writes its `self` without checking what it is writes past every
# refusal (`BaseException.with_traceback(read, None)` emptied a host
# function's closure; `BaseException.add_note` gave a struct sequence a
# list).
#
# The prelude's classes are swept too (`PRELUDE`): their methods are Python,
# and a `self.x = v` in one, called through its class with a shared object
# as `self`, wrote where the same statement in a cell is refused
# (`json.JSONEncoder.__init__(json.JSONEncoder, default=f)` made every later
# encoder call `f`). They are called with keyword arguments as well (each
# method's keyword-only names and `KEYWORDS`), and instances made from the
# shared objects have their methods called (`types.GenericAlias(lambda:
# Entry, ...)()` gave what its origin returned an attribute).
#
# `MODE = 'call'` makes the calls; `MODE = 'check'` reports the shared
# objects' state. A cell that calls, a `reset()`, and a cell that checks must
# print what the check prints in a new session.
#
# The types are CPython 3.14's `dir(builtins)` that are types, then the
# types without a builtin name; one this build does not have is skipped.

import sys, json, asyncio, types, fs


def _gen():
    yield 1


async def _agen():
    yield 1


async def _coro():
    return 1


def _tb():
    try:
        1 / 0
    except ZeroDivisionError as e:
        return e.__traceback__


def _cell():
    x = 1
    return (lambda: x).__closure__[0]


TYPES = [
    lambda: ArithmeticError, lambda: AssertionError, lambda: AttributeError, lambda: BaseException,
    lambda: BaseExceptionGroup, lambda: BlockingIOError, lambda: BrokenPipeError, lambda:
    BufferError, lambda: BytesWarning, lambda: ChildProcessError, lambda: ConnectionAbortedError,
    lambda: ConnectionError, lambda: ConnectionRefusedError, lambda: ConnectionResetError, lambda:
    DeprecationWarning, lambda: EOFError, lambda: EncodingWarning, lambda: EnvironmentError, lambda:
    Exception, lambda: ExceptionGroup, lambda: FileExistsError, lambda: FileNotFoundError, lambda:
    FloatingPointError, lambda: FutureWarning, lambda: GeneratorExit, lambda: IOError, lambda:
    ImportError, lambda: ImportWarning, lambda: IndentationError, lambda: IndexError, lambda:
    InterruptedError, lambda: IsADirectoryError, lambda: KeyError, lambda: KeyboardInterrupt,
    lambda: LookupError, lambda: MemoryError, lambda: ModuleNotFoundError, lambda: NameError,
    lambda: NotADirectoryError, lambda: NotImplementedError, lambda: OSError, lambda: OverflowError,
    lambda: PendingDeprecationWarning, lambda: PermissionError, lambda: ProcessLookupError, lambda:
    PythonFinalizationError, lambda: RecursionError, lambda: ReferenceError, lambda:
    ResourceWarning, lambda: RuntimeError, lambda: RuntimeWarning, lambda: StopAsyncIteration,
    lambda: StopIteration, lambda: SyntaxError, lambda: SyntaxWarning, lambda: SystemError, lambda:
    SystemExit, lambda: TabError, lambda: TimeoutError, lambda: TypeError, lambda:
    UnboundLocalError, lambda: UnicodeDecodeError, lambda: UnicodeEncodeError, lambda: UnicodeError,
    lambda: UnicodeTranslateError, lambda: UnicodeWarning, lambda: UserWarning, lambda: ValueError,
    lambda: Warning, lambda: ZeroDivisionError, lambda: _IncompleteInputError, lambda: __loader__,
    lambda: bool, lambda: bytearray, lambda: bytes, lambda: classmethod, lambda: complex, lambda:
    dict, lambda: enumerate, lambda: filter, lambda: float, lambda: frozenset, lambda: int, lambda:
    list, lambda: map, lambda: memoryview, lambda: object, lambda: property, lambda: range, lambda:
    reversed, lambda: set, lambda: slice, lambda: staticmethod, lambda: str, lambda: super, lambda:
    tuple, lambda: type, lambda: zip,
    lambda: type(len), lambda: type(_gen), lambda: type(sys), lambda: type(_gen()), lambda: type(_agen()),
    lambda: type(_coro()), lambda: type(iter([])), lambda: type(iter(())), lambda: type(iter({})),
    lambda: type(iter(range(1))), lambda: type({}.keys()), lambda: type({}.values()), lambda:
    type({}.items()), lambda: type(None), lambda: type(NotImplemented), lambda: type(Ellipsis), lambda:
    type(_tb()), lambda: type(_tb().tb_frame), lambda: type(_gen.__code__), lambda: type(_cell()),
    lambda: type(int.__add__), lambda: type([].append), lambda: type(int.real), lambda:
    type(type.__dict__), lambda: type(Entry.go), lambda: type(obj().go), lambda: type(json.dumps),
    lambda: type(asyncio.Lock().acquire), lambda: type(sys.version_info), lambda: type(CONFR),
]

# Every class the prelude defines (`^class` in `mm-engine/src/prelude.py`),
# as a cell reaches it.
PRELUDE = [
    ('json.JSONDecodeError', lambda: json.JSONDecodeError), ('json.JSONDecoder', lambda: json.JSONDecoder),
    ('json.JSONEncoder', lambda: json.JSONEncoder), ('asyncio.Semaphore', lambda: asyncio.Semaphore),
    ('asyncio.BoundedSemaphore', lambda: asyncio.BoundedSemaphore), ('asyncio.Lock', lambda: asyncio.Lock),
    ('asyncio.Event', lambda: asyncio.Event), ('asyncio.Condition', lambda: asyncio.Condition),
    ('asyncio.Barrier', lambda: asyncio.Barrier), ('asyncio.BrokenBarrierError', lambda: asyncio.BrokenBarrierError),
    ('asyncio.Runner', lambda: asyncio.Runner), ('as_completed', lambda: type(asyncio.as_completed([]))),
    ('asyncio.QueueEmpty', lambda: asyncio.QueueEmpty), ('asyncio.QueueFull', lambda: asyncio.QueueFull),
    ('asyncio.QueueShutDown', lambda: asyncio.QueueShutDown), ('asyncio.Queue', lambda: asyncio.Queue),
    ('asyncio.PriorityQueue', lambda: asyncio.PriorityQueue), ('asyncio.LifoQueue', lambda: asyncio.LifoQueue),
    ('asyncio.Handle', lambda: asyncio.Handle), ('asyncio.TimerHandle', lambda: asyncio.TimerHandle),
    ('event loop', lambda: type(asyncio.new_event_loop())), ('asyncio.TaskGroup', lambda: asyncio.TaskGroup),
    ('asyncio.Timeout', lambda: asyncio.Timeout), ('struct sequence', lambda: type(sys.version_info)),
    ('types.GenericAlias', lambda: types.GenericAlias), ('types.UnionType', lambda: types.UnionType),
    ('globals view', lambda: type(globals())), ('coroutine wrapper', lambda: type(_coro().__await__())),
    ('BaseExceptionGroup', lambda: BaseExceptionGroup), ('ExceptionGroup', lambda: ExceptionGroup),
]

# Instance methods not called: they run the loop for good with nothing to
# stop it.
SKIP = ['run_forever']

# Keyword names tried on every method, beside its own keyword-only ones.
KEYWORDS = ['default', 'object_hook', 'maxsize', 'value', 'name', 'loop', 'context']

# What a reset shares: host definitions, the prelude's functions, classes
# and struct sequences, and native types and functions.
TARGETS = [
    ('read', lambda: read), ('h', lambda: h), ('tool', lambda: tool), ('fs.read', lambda: fs.read),
    ('fs.raw', lambda: fs.raw), ('Entry', lambda: Entry), ('Entry.go', lambda: Entry.go),
    ('Entry.path', lambda: Entry.__dict__['path']), ('Entry.__new__', lambda: Entry.__dict__['__new__']),
    ('fs.Node', lambda: fs.Node), ('ToolError', lambda: ToolError), ('fs.FsError', lambda: fs.FsError),
    ('sys.version_info', lambda: sys.version_info), ('sys.float_info', lambda: sys.float_info),
    ('json.dumps', lambda: json.dumps), ('json.JSONDecodeError', lambda: json.JSONDecodeError),
    ('asyncio.Lock', lambda: asyncio.Lock), ('MemoryError', lambda: MemoryError), ('int', lambda: int),
    ('object', lambda: object), ('len', lambda: len), ('list.append', lambda: list.append),
    ('record class', lambda: type(CONFR)), ('json.JSONEncoder', lambda: json.JSONEncoder),
    ('asyncio.Queue', lambda: asyncio.Queue), ('types.GenericAlias', lambda: types.GenericAlias),
]


def _made(name, make):
    try:
        return [make()]
    except BaseException:
        return []


def _attempt(f, *a, **k):
    try:
        f(*a, **k)
        return 1
    except BaseException:
        return 0


def call():
    tb, g = _tb(), _gen()
    argsets = [(), (None,), ('x',), ('x', 1), (tb,), ({'x': 1},), (['x'],), (0, 'x'),
               ('__doc__', 'evil'), (('x',),), ('x', ('a',), (g,))]
    targets = [t() for _, t in TARGETS]
    prelude = [T for name, make in PRELUDE for T in _made(name, make)]
    seen = []
    types = methods = done = 0
    for T in [T for make in TYPES for T in _made('', make)] + prelude:
        types += 1
        for name in sorted(dir(T)):
            try:
                m = getattr(T, name)
            except BaseException:
                continue
            if not callable(m) or any(m is s for s in seen):
                continue
            seen.append(m)
            methods += 1
            own = _try(lambda: list(m.__kwdefaults__ or ()))
            keys = KEYWORDS + (own if type(own) is list else [])
            for tg in targets:
                for a in argsets:
                    done += _attempt(m, tg, *a)
                for k in keys:
                    done += _attempt(m, tg, **{k: g})
                done += _attempt(m, tg, **{k: g for k in keys})
    # Instances made from the shared objects, and their methods.
    made = 0
    for T in prelude:
        for tg in targets:
            for args in ((tg,), (lambda: tg, (g,)), (tg, tg)):
                for o in _made('', lambda: T(*args)):
                    made += 1
                    for name in sorted(dir(o)):
                        if name in SKIP:
                            continue
                        for m in _made('', lambda: getattr(o, name)):
                            if callable(m):
                                _attempt(m)
                                _attempt(m, tg)
    print('types', types, 'methods', methods, 'returned', done > 1000, 'instances', made > 100)


def _try(f):
    try:
        return f()
    except BaseException as e:
        return type(e).__name__ + ': ' + str(e)


def _raised():
    try:
        boom()
    except ToolError as e:
        return [e.args, e.tool, e.data, getattr(e, '__notes__', None), e.__traceback__ is not None]


def check():
    for name, t in TARGETS:
        o = t()
        d = getattr(type(o), '__dict__', None) if not isinstance(o, type) else o.__dict__
        print(name, _try(lambda: sorted(dir(o))), _try(lambda: sorted(d)),
              _try(lambda: getattr(o, '__notes__', None)), _try(lambda: o.__name__),
              _try(lambda: o.__doc__), _try(lambda: o.__mro__))
    for name, make in PRELUDE:
        for T in _made(name, make):
            print(name, _try(lambda: sorted(T.__dict__)), _try(lambda: T.__name__), _try(lambda: T.__mro__))
    for f in (lambda: json.JSONEncoder().encode({'a': [1]}), lambda: json.JSONDecoder().decode('{"a": 1}'),
              lambda: json.dumps({'a': {1}}), lambda: asyncio.Queue().maxsize, lambda: repr(list[int]),
              lambda: sorted(dir(types.GenericAlias(list, int))), lambda: readr(), lambda: obj().gor(),
              lambda: sorted(dir(asyncio.new_event_loop())), lambda: asyncio.get_event_loop()):
        print(_try(f))
    for f in (lambda: read(), lambda: h(), lambda: tool(1), lambda: fs.read(), lambda: fs.raw(2),
              lambda: obj().go(), lambda: repr(obj()), lambda: Entry('q').path, lambda: Entry('q').go(3),
              lambda: fs.Node(), _raised, lambda: json.dumps({'a': [1]}), lambda: json.loads('[1'),
              lambda: sorted(vars(asyncio.Lock)), lambda: asyncio.run(_coro()),
              lambda: list(sys.version_info), lambda: sys.float_info.dig, lambda: str(MemoryError('m')),
              lambda: int('7'), lambda: len('ab'), lambda: object().__class__.__name__):
        print(_try(f))


call() if MODE == 'call' else check()
