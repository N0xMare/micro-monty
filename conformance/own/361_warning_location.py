# Warnings name the frame CPython's _warnings attributes them to: the
# caller's filename (== __file__), line and module, for stacklevel 1, 2,
# off the stack (<sys>:0), 0 and negative, skip_file_prefixes, calls
# through builtins (map), dunders and generators, and the warnings re and
# datetime emit for the user's call (docs/spec.md §12). The registry lives
# in the caller's globals. Shown warnings go to stderr, which is not
# compared.
import warnings, re, datetime


def f():
    warnings.warn("lvl1")


def g():
    warnings.warn("lvl2", stacklevel=2)


def h():
    g()


class C:
    def __eq__(self, o):
        warnings.warn("eq", stacklevel=2)
        return True


def cmp():
    return C() == 1


def gen():
    warnings.warn("gen", stacklevel=2)
    yield 1


def k():
    warnings.warn("skip", stacklevel=1, skip_file_prefixes=(__file__,))


def k2():
    warnings.warn("skip2", skip_file_prefixes=("<nothing>",))


with warnings.catch_warnings(record=True) as log:
    warnings.simplefilter("always")
    f()
    h()
    warnings.warn("top")
    warnings.warn("off", stacklevel=5)
    warnings.warn("zero", stacklevel=0)
    warnings.warn("neg", stacklevel=-3)
    re.sub(r'\d', 'x', '12',
           1)
    re.split('a', 'bab', 1)
    re.compile(r'[0-9--1]')
    re.findall(r'[[a]', 'a')
    re.compile('x').sub('y', 'xx', 1)
    datetime.datetime.strptime("1 2", "%d %m")
    datetime.datetime.utcnow()
    k()
    k2()
    list(map(lambda x: warnings.warn("map"), [1]))
    sorted([2, 1], key=lambda x: warnings.warn("key", stacklevel=1) or x)
    cmp()
    for _ in gen():
        pass
for w in log:
    print(w.filename == __file__, w.filename if w.filename != __file__ else "", w.lineno,
          w.category.__name__, str(w.message)[:30], w.line, w.file)

with warnings.catch_warnings(record=True) as log:
    warnings.resetwarnings()
    warnings.simplefilter("default")
    for i in range(3):
        f()
        warnings.warn("loop")
    warnings.simplefilter("module")
    warnings.warn("mod")
    warnings.warn("mod")
    warnings.filterwarnings("error", module="__main__", lineno=0, message="raise")
    try:
        warnings.warn("raise me")
    except UserWarning as e:
        print("error filter on module", e)
    warnings.filterwarnings("ignore", module="sys")
    warnings.warn("hidden off the stack", stacklevel=9)
print([(w.lineno, str(w.message)) for w in log])
print("__warningregistry__" in dir(), type(__warningregistry__).__name__)
print(sorted(k for k in __warningregistry__ if k != "version")[:2])
