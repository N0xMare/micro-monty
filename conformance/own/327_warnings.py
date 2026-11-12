# warnings: the builtin Warning hierarchy and the warnings module's filters,
# catch_warnings, WarningMessage, formatwarning and deprecated (docs/spec.md
# §4, §12). Shown warnings go to stderr, which is not compared; where a
# warning is attributed is 361_warning_location.py's subject.
import warnings
print(issubclass(DeprecationWarning, Warning), Warning.__mro__)
for c in (Warning, DeprecationWarning, PendingDeprecationWarning, RuntimeWarning, SyntaxWarning, UserWarning,
          FutureWarning, ImportWarning, UnicodeWarning, BytesWarning, ResourceWarning, EncodingWarning):
    print(c.__name__, c.__bases__, c.__module__, isinstance(c("m"), Exception), repr(c("m", 2)))
print(warnings.filters)
print(warnings.__all__)
with warnings.catch_warnings(record=True) as w:
    warnings.simplefilter("always")
    warnings.warn("hello")
    warnings.warn("dep", DeprecationWarning)
    warnings.warn(RuntimeWarning("inst"))
    print(len(w), [(type(x.message).__name__, str(x.message), x.category.__name__) for x in w])
    print(w[0].filename == w[1].filename, type(w[0].lineno).__name__, w[0].file, w[0].line, w[0].source)
with warnings.catch_warnings():
    warnings.simplefilter("error")
    try:
        warnings.warn("boom", FutureWarning)
    except FutureWarning as e:
        print("raised", type(e).__name__, e)
with warnings.catch_warnings(record=True) as w:
    warnings.simplefilter("ignore")
    warnings.warn("x")
    print(w)
with warnings.catch_warnings(record=True) as w:
    warnings.simplefilter("always")
    warnings.filterwarnings("ignore", category=UserWarning)
    warnings.filterwarnings("error", message="bad.*", category=RuntimeWarning)
    warnings.warn("u")
    warnings.warn("ok", RuntimeWarning)
    try:
        warnings.warn("BAD thing", RuntimeWarning)
    except RuntimeWarning as e:
        print("err", e)
    print([str(x.message) for x in w])
with warnings.catch_warnings(record=True) as w:
    warnings.simplefilter("default")
    for i in range(3):
        warnings.warn("same")
    warnings.simplefilter("once")
    print(len(w))
with warnings.catch_warnings(record=True) as w:
    warnings.warn("plain dep", DeprecationWarning)
    warnings.warn("pending", PendingDeprecationWarning)
    print([x.category.__name__ for x in w])
print(len(warnings.filters))
with warnings.catch_warnings(action="ignore"):
    print(warnings.filters[0][:1])
print(warnings.formatwarning("msg", UserWarning, "f.py", 3), end="")
print(warnings.formatwarning("msg", UserWarning, "f.py", 3, "  x = 1  "), end="")
m = warnings.WarningMessage("m", UserWarning, "f.py", 3)
print(str(m))
print(repr(warnings.catch_warnings(record=True)), repr(warnings.catch_warnings()))
for bad in [lambda: warnings.warn("x", int), lambda: warnings.simplefilter("nope"),
            lambda: warnings.filterwarnings("ignore", category=int),
            lambda: warnings.warn("x", skip_file_prefixes=["a"]),
            lambda: warnings.simplefilter("ignore", lineno=-1)]:
    try:
        bad()
    except (TypeError, ValueError) as e:
        print(type(e).__name__, e)
got = []
def sw(message, category, filename, lineno, file=None, line=None):
    got.append((str(message), category.__name__))
with warnings.catch_warnings():
    warnings.simplefilter("always")
    warnings.showwarning = sw
    warnings.warn("custom")
print(got, warnings.showwarning is sw)
import io
buf = io.StringIO()
warnings.showwarning("shown", UserWarning, "<sys>", 0, file=buf)
print(repr(buf.getvalue()))
@warnings.deprecated("use g")
def f(x):
    return x * 2
print(f.__deprecated__, f.__name__)
with warnings.catch_warnings(record=True) as w:
    warnings.simplefilter("always")
    print(f(3), [(str(x.message), x.category.__name__) for x in w])
@warnings.deprecated("old class")
class A:
    def __init__(self, v):
        self.v = v
with warnings.catch_warnings(record=True) as w:
    warnings.simplefilter("always")
    print(A(5).v, A.__deprecated__)
    class B(A):
        pass
    print(B(1).v, [str(x.message) for x in w])
try:
    warnings.deprecated(5)
except TypeError as e:
    print(e)
@warnings.deprecated("hooked")
class H:
    seen = []
    def __init_subclass__(cls, tag=None, **kw):
        H.seen.append((cls.__name__, tag))
with warnings.catch_warnings(record=True) as w:
    warnings.simplefilter("always")
    class K(H, tag="t"):
        pass
    print(H.seen, [str(x.message) for x in w], type(K()).__name__)
class MyWarning(UserWarning):
    pass
with warnings.catch_warnings(record=True) as w:
    warnings.simplefilter("ignore")
    warnings.simplefilter("always", MyWarning)
    warnings.warn("mine", MyWarning)
    warnings.warn("plain")
    print([(x.category.__name__, str(x.message)) for x in w])
try:
    raise DeprecationWarning("as exception", 1)
except Warning as e:
    print(type(e).__name__, e.args, isinstance(e, Exception))
