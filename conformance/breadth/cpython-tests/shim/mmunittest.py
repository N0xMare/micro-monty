# mmunittest: a minimal unittest + test.support shim written in the
# micro-monty subset. harness.py prepends this file to every transformed
# CPython test module, so the identical source runs on CPython 3.14 and on mm.
# Every name it defines starts with `_mm`; the transformed test binds
# `unittest`, `support` and the support submodules to the namespaces below.
#
# Result protocol (stderr, which mm does not buffer): before each method
#   @@MMSTART\t<Class.method>
# and after it
#   @@MMT\t<Class.method>\t<status>\t<exc type>\t<line>\t<message>
# with status one of pass, fail, error, skip, xfail, xpass.
# A top-level statement that raised prints
#   @@MMTOP\t<line>\t<exc type>\t<message>

import sys as _mm_sys

_mm_stderr = _mm_sys.stderr
_MM_IS_MM = "micro-monty" in _mm_sys.version


def _mm_emit(*parts):
    out = []
    for p in parts:
        try:
            s = str(p)
        except BaseException as e:
            s = "<str failed: " + type(e).__name__ + ">"
        if len(s) > 400 and parts[0] != "@@MMCLASS":
            s = s[:400] + "..."
        s = s.replace("\\", "\\\\").replace("\t", "\\t").replace("\r", "\\r").replace("\n", "\\n")
        out.append(s)
    _mm_stderr.write("\t".join(out) + "\n")


class _MMShimMissing(Exception):
    pass


class _MMExcluded(Exception):
    pass


def _mm_excluded(msg):
    return _MMExcluded(msg)


class _MMMissing:
    def __init__(self, name):
        object.__setattr__(self, "_mm_name", name)

    def __getattr__(self, attr):
        if attr.startswith("__"):
            raise AttributeError(attr)
        return _MMMissing(self._mm_name + "." + attr)

    def __call__(self, *args, **kwargs):
        raise _MMShimMissing("not available: " + self._mm_name)

    def __repr__(self):
        return "<missing " + self._mm_name + ">"


def _mm_missing(name):
    return _MMMissing(name)


class _MMMissingModuleError(Exception):
    pass


class _MMMissingModule:
    # bound by the harness when a top-level import fails
    def __init__(self, name, why):
        object.__setattr__(self, "_mm_name", name)
        object.__setattr__(self, "_mm_why", why)

    def __getattr__(self, attr):
        if attr.startswith("__"):
            raise AttributeError(attr)
        raise _MMMissingModuleError("import failed: " + self._mm_why)

    def __call__(self, *args, **kwargs):
        raise _MMMissingModuleError("import failed: " + self._mm_why)


def _mm_missing_module(name, why):
    return _MMMissingModule(name, why)


class _MMExcludedClass:
    def __init__(self, msg):
        object.__setattr__(self, "_mm_msg", msg)

    def __getattr__(self, attr):
        if attr.startswith("__"):
            raise AttributeError(attr)
        raise _MMExcluded(self._mm_msg)

    def __call__(self, *args, **kwargs):
        raise _MMExcluded(self._mm_msg)


def _mm_excluded_class(msg):
    return _MMExcludedClass(msg)


def _mm_toplevel_error(line, e):
    _mm_emit("@@MMTOP", line, type(e).__name__, e)


class _MMNamespace:
    def __init__(self, name):
        object.__setattr__(self, "_mm_name", name)

    def __getattr__(self, attr):
        if attr.startswith("__"):
            raise AttributeError(attr)
        return _MMMissing(self._mm_name + "." + attr)


def _mm_from(ns, attr):
    try:
        return getattr(ns, attr)
    except AttributeError:
        return _MMMissing(attr)


# ---------------------------------------------------------------- unittest

class _MMSkipTest(Exception):
    pass


class _MMSubTestFailure(Exception):
    pass


def _mm_skip(reason):
    def deco(obj):
        obj.__unittest_skip__ = True
        obj.__unittest_skip_why__ = reason
        return obj
    if callable(reason) and not isinstance(reason, str):
        raise TypeError("skip() needs a reason")
    return deco


def _mm_identity(obj):
    return obj


def _mm_skip_if(condition, reason):
    if condition:
        return _mm_skip(reason)
    return _mm_identity


def _mm_skip_unless(condition, reason):
    if not condition:
        return _mm_skip(reason)
    return _mm_identity


def _mm_expected_failure(f):
    f.__unittest_expecting_failure__ = True
    return f


def _mm_safe_repr(obj):
    try:
        r = repr(obj)
    except BaseException as e:
        r = "<repr failed: " + type(e).__name__ + ">"
    if len(r) > 200:
        r = r[:200] + "..."
    return r


class _MMRaisesContext:
    def __init__(self, test, expected, regex=None, msg=None):
        self.test = test
        self.expected = expected
        self.regex = regex
        self.msg = msg
        self.exception = None

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, tb):
        if exc_type is None:
            exp = self.expected
            if isinstance(exp, tuple):
                name = str(tuple(x.__name__ for x in exp))
            else:
                name = getattr(exp, "__name__", str(exp))
            self.test.fail(self.msg or (name + " not raised"))
        if not issubclass(exc_type, self.expected):
            return False
        self.exception = exc_value
        if self.regex is not None:
            import re
            pat = self.regex
            if isinstance(pat, str):
                pat = re.compile(pat)
            if not pat.search(str(exc_value)):
                self.test.fail('"%s" does not match "%s"' % (pat.pattern, str(exc_value)))
        return True


class _MMWarnsContext:
    def __init__(self, test, expected, regex=None):
        import warnings
        self.warnings = warnings
        self.test = test
        self.expected = expected
        self.regex = regex
        self.warning = None
        self.filename = None
        self.lineno = None

    def __enter__(self):
        self.cm = self.warnings.catch_warnings(record=True)
        self.log = self.cm.__enter__()
        self.warnings.simplefilter("always")
        return self

    def __exit__(self, exc_type, exc_value, tb):
        self.cm.__exit__(exc_type, exc_value, tb)
        if exc_type is not None:
            return False
        import re
        for w in self.log:
            if not issubclass(w.category, self.expected):
                continue
            if self.regex is not None and not re.search(self.regex, str(w.message)):
                continue
            self.warning = w.message
            self.filename = w.filename
            self.lineno = w.lineno
            return None
        self.test.fail("%s not triggered" % (getattr(self.expected, "__name__", self.expected),))


class _MMSubTest:
    def __init__(self, test, msg, params):
        self.test = test
        self.msg = msg
        self.params = params

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, tb):
        if exc_type is None:
            return False
        if issubclass(exc_type, _MMSkipTest):
            return True
        if not issubclass(exc_type, Exception):
            return False
        if self.test._mm_subfailure is None:
            self.test._mm_subfailure = exc_value
        return True


class _MMLogsContext:
    def __init__(self, test, logger, level, no_logs):
        self.test = test
        self.logger_name = logger
        self.level = level
        self.no_logs = no_logs

    def __enter__(self):
        import logging
        test_ctx = self

        class _Capture(logging.Handler):
            def emit(self, record):
                test_ctx.records.append(record)
                test_ctx.output.append("%s:%s:%s" % (record.levelname, record.name, record.getMessage()))

        if isinstance(self.logger_name, logging.Logger):
            logger = self.logger_name
        else:
            logger = logging.getLogger(self.logger_name)
        self.logger = logger
        self.records = []
        self.output = []
        level = self.level
        if level is None:
            level = logging.INFO
        elif isinstance(level, str):
            level = getattr(logging, level)
        self.handler = _Capture()
        self.old = (logger.handlers[:], logger.level, logger.propagate)
        logger.handlers = [self.handler]
        logger.setLevel(level)
        logger.propagate = False
        if self.no_logs:
            return None
        return self

    def __exit__(self, exc_type, exc_value, tb):
        self.logger.handlers = self.old[0]
        self.logger.setLevel(self.old[1])
        self.logger.propagate = self.old[2]
        if exc_type is not None:
            return False
        if self.no_logs:
            if self.output:
                self.test.fail("Unexpected logs found: %r" % (self.output,))
        elif not self.output:
            self.test.fail("no logs of level %s or higher triggered on %s" % (self.level, self.logger.name))
        return False


class _MMTestCase:
    failureException = AssertionError
    longMessage = True
    maxDiff = 80 * 8
    _mm_subfailure = None

    def __init__(self, methodName="runTest"):
        self._testMethodName = methodName
        self._cleanups = []
        self._mm_subfailure = None

    def id(self):
        return type(self).__name__ + "." + self._testMethodName

    def __str__(self):
        return "%s (%s)" % (self._testMethodName, type(self).__name__)

    def __repr__(self):
        return "<%s testMethod=%s>" % (type(self).__name__, self._testMethodName)

    def shortDescription(self):
        return None

    def setUp(self):
        pass

    def tearDown(self):
        pass

    @classmethod
    def setUpClass(cls):
        pass

    @classmethod
    def tearDownClass(cls):
        pass

    @classmethod
    def addClassCleanup(cls, function, *args, **kwargs):
        if "_mm_class_cleanups" not in cls.__dict__:
            cls._mm_class_cleanups = []
        cls._mm_class_cleanups.append((function, args, kwargs))

    @classmethod
    def doClassCleanups(cls):
        lst = cls.__dict__.get("_mm_class_cleanups")
        while lst:
            function, args, kwargs = lst.pop()
            function(*args, **kwargs)

    @classmethod
    def enterClassContext(cls, cm):
        result = type(cm).__enter__(cm)
        cls.addClassCleanup(type(cm).__exit__, cm, None, None, None)
        return result

    def addCleanup(self, function, *args, **kwargs):
        self._cleanups.append((function, args, kwargs))

    def enterContext(self, cm):
        result = type(cm).__enter__(cm)
        self.addCleanup(type(cm).__exit__, cm, None, None, None)
        return result

    def doCleanups(self):
        first = None
        while self._cleanups:
            function, args, kwargs = self._cleanups.pop()
            try:
                function(*args, **kwargs)
            except Exception as e:
                if first is None:
                    first = e
        if first is not None:
            raise first

    def skipTest(self, reason):
        raise _MMSkipTest(reason)

    def subTest(self, msg=None, **params):
        return _MMSubTest(self, msg, params)

    def fail(self, msg=None):
        raise self.failureException(msg)

    def _mm_msg(self, msg, standard):
        if msg is None:
            return standard
        return standard + " : " + str(msg)

    def assertTrue(self, expr, msg=None):
        if not expr:
            self.fail(self._mm_msg(msg, "%s is not true" % _mm_safe_repr(expr)))

    def assertFalse(self, expr, msg=None):
        if expr:
            self.fail(self._mm_msg(msg, "%s is not false" % _mm_safe_repr(expr)))

    def assertEqual(self, first, second, msg=None):
        if not first == second:
            self.fail(self._mm_msg(msg, "%s != %s" % (_mm_safe_repr(first), _mm_safe_repr(second))))

    def assertNotEqual(self, first, second, msg=None):
        if not first != second:
            self.fail(self._mm_msg(msg, "%s == %s" % (_mm_safe_repr(first), _mm_safe_repr(second))))

    def assertMultiLineEqual(self, first, second, msg=None):
        if not isinstance(first, str):
            self.fail(self._mm_msg(msg, "First argument is not a string"))
        if not isinstance(second, str):
            self.fail(self._mm_msg(msg, "Second argument is not a string"))
        self.assertEqual(first, second, msg)

    def _mm_typed_equal(self, first, second, msg, seq_type):
        if seq_type is not None:
            if not isinstance(first, seq_type):
                self.fail(self._mm_msg(msg, "First sequence is not a %s: %s" % (seq_type.__name__, _mm_safe_repr(first))))
            if not isinstance(second, seq_type):
                self.fail(self._mm_msg(msg, "Second sequence is not a %s: %s" % (seq_type.__name__, _mm_safe_repr(second))))
        self.assertEqual(first, second, msg)

    def assertSequenceEqual(self, seq1, seq2, msg=None, seq_type=None):
        if seq_type is not None:
            self._mm_typed_equal(seq1, seq2, msg, seq_type)
            return
        l1 = len(seq1)
        l2 = len(seq2)
        if l1 != l2:
            self.fail(self._mm_msg(msg, "sequences differ in length: %d != %d" % (l1, l2)))
        i = 0
        while i < l1:
            if not seq1[i] == seq2[i]:
                self.fail(self._mm_msg(msg, "sequences differ at index %d: %s != %s" % (i, _mm_safe_repr(seq1[i]), _mm_safe_repr(seq2[i]))))
            i += 1

    def assertListEqual(self, first, second, msg=None):
        self._mm_typed_equal(first, second, msg, list)

    def assertTupleEqual(self, first, second, msg=None):
        self._mm_typed_equal(first, second, msg, tuple)

    def assertDictEqual(self, first, second, msg=None):
        self._mm_typed_equal(first, second, msg, dict)

    def assertSetEqual(self, first, second, msg=None):
        try:
            d1 = first.difference(second)
            d2 = second.difference(first)
        except (TypeError, AttributeError) as e:
            self.fail(self._mm_msg(msg, "invalid set: %s" % e))
        if d1 or d2:
            self.fail(self._mm_msg(msg, "sets differ: %s / %s" % (_mm_safe_repr(d1), _mm_safe_repr(d2))))

    def assertCountEqual(self, first, second, msg=None):
        a = list(first)
        b = list(second)
        missing = []
        for x in a:
            found = False
            i = 0
            while i < len(b):
                if b[i] == x:
                    del b[i]
                    found = True
                    break
                i += 1
            if not found:
                missing.append(x)
        if missing or b:
            self.fail(self._mm_msg(msg, "Element counts were not equal: %s / %s" % (_mm_safe_repr(missing), _mm_safe_repr(b))))

    def assertIs(self, first, second, msg=None):
        if first is not second:
            self.fail(self._mm_msg(msg, "%s is not %s" % (_mm_safe_repr(first), _mm_safe_repr(second))))

    def assertIsNot(self, first, second, msg=None):
        if first is second:
            self.fail(self._mm_msg(msg, "unexpectedly identical: %s" % _mm_safe_repr(first)))

    def assertIsNone(self, obj, msg=None):
        if obj is not None:
            self.fail(self._mm_msg(msg, "%s is not None" % _mm_safe_repr(obj)))

    def assertIsNotNone(self, obj, msg=None):
        if obj is None:
            self.fail(self._mm_msg(msg, "unexpectedly None"))

    def assertIn(self, member, container, msg=None):
        if member not in container:
            self.fail(self._mm_msg(msg, "%s not found in %s" % (_mm_safe_repr(member), _mm_safe_repr(container))))

    def assertNotIn(self, member, container, msg=None):
        if member in container:
            self.fail(self._mm_msg(msg, "%s unexpectedly found in %s" % (_mm_safe_repr(member), _mm_safe_repr(container))))

    def assertIsInstance(self, obj, cls, msg=None):
        if not isinstance(obj, cls):
            self.fail(self._mm_msg(msg, "%s is not an instance of %r" % (_mm_safe_repr(obj), cls)))

    def assertNotIsInstance(self, obj, cls, msg=None):
        if isinstance(obj, cls):
            self.fail(self._mm_msg(msg, "%s is an instance of %r" % (_mm_safe_repr(obj), cls)))

    def assertIsSubclass(self, cls, superclass, msg=None):
        try:
            if issubclass(cls, superclass):
                return
        except TypeError:
            if not isinstance(cls, type):
                self.fail(self._mm_msg(msg, "%r is not a class" % (cls,)))
            raise
        self.fail(self._mm_msg(msg, "%r is not a subclass of %r" % (cls, superclass)))

    def assertNotIsSubclass(self, cls, superclass, msg=None):
        if issubclass(cls, superclass):
            self.fail(self._mm_msg(msg, "%r is a subclass of %r" % (cls, superclass)))

    def assertHasAttr(self, obj, name, msg=None):
        if not hasattr(obj, name):
            self.fail(self._mm_msg(msg, "%s has no attribute %r" % (_mm_safe_repr(obj), name)))

    def assertNotHasAttr(self, obj, name, msg=None):
        if hasattr(obj, name):
            self.fail(self._mm_msg(msg, "%s has unexpected attribute %r" % (_mm_safe_repr(obj), name)))

    def assertStartsWith(self, s, prefix, msg=None):
        if not s.startswith(prefix):
            self.fail(self._mm_msg(msg, "%s doesn't start with %s" % (_mm_safe_repr(s), _mm_safe_repr(prefix))))

    def assertNotStartsWith(self, s, prefix, msg=None):
        if s.startswith(prefix):
            self.fail(self._mm_msg(msg, "%s starts with %s" % (_mm_safe_repr(s), _mm_safe_repr(prefix))))

    def assertEndsWith(self, s, suffix, msg=None):
        if not s.endswith(suffix):
            self.fail(self._mm_msg(msg, "%s doesn't end with %s" % (_mm_safe_repr(s), _mm_safe_repr(suffix))))

    def assertNotEndsWith(self, s, suffix, msg=None):
        if s.endswith(suffix):
            self.fail(self._mm_msg(msg, "%s ends with %s" % (_mm_safe_repr(s), _mm_safe_repr(suffix))))

    def assertGreater(self, a, b, msg=None):
        if not a > b:
            self.fail(self._mm_msg(msg, "%s not greater than %s" % (_mm_safe_repr(a), _mm_safe_repr(b))))

    def assertGreaterEqual(self, a, b, msg=None):
        if not a >= b:
            self.fail(self._mm_msg(msg, "%s not greater than or equal to %s" % (_mm_safe_repr(a), _mm_safe_repr(b))))

    def assertLess(self, a, b, msg=None):
        if not a < b:
            self.fail(self._mm_msg(msg, "%s not less than %s" % (_mm_safe_repr(a), _mm_safe_repr(b))))

    def assertLessEqual(self, a, b, msg=None):
        if not a <= b:
            self.fail(self._mm_msg(msg, "%s not less than or equal to %s" % (_mm_safe_repr(a), _mm_safe_repr(b))))

    def assertAlmostEqual(self, first, second, places=None, msg=None, delta=None):
        if first == second:
            return
        if delta is not None and places is not None:
            raise TypeError("specify delta or places not both")
        diff = abs(first - second)
        if delta is not None:
            if diff <= delta:
                return
            standard = "%s != %s within %s delta (%s difference)" % (_mm_safe_repr(first), _mm_safe_repr(second), _mm_safe_repr(delta), _mm_safe_repr(diff))
        else:
            if places is None:
                places = 7
            if round(diff, places) == 0:
                return
            standard = "%s != %s within %r places (%s difference)" % (_mm_safe_repr(first), _mm_safe_repr(second), places, _mm_safe_repr(diff))
        self.fail(self._mm_msg(msg, standard))

    def assertNotAlmostEqual(self, first, second, places=None, msg=None, delta=None):
        if delta is not None and places is not None:
            raise TypeError("specify delta or places not both")
        diff = abs(first - second)
        if delta is not None:
            if not (first == second) and diff > delta:
                return
        else:
            if places is None:
                places = 7
            if not (first == second) and round(diff, places) != 0:
                return
        self.fail(self._mm_msg(msg, "%s == %s" % (_mm_safe_repr(first), _mm_safe_repr(second))))

    def assertRegex(self, text, expected_regex, msg=None):
        import re
        if isinstance(expected_regex, str):
            expected_regex = re.compile(expected_regex)
        if not expected_regex.search(text):
            self.fail(self._mm_msg(msg, "Regex didn't match: %r not found in %r" % (expected_regex.pattern, text)))

    def assertNotRegex(self, text, unexpected_regex, msg=None):
        import re
        if isinstance(unexpected_regex, str):
            unexpected_regex = re.compile(unexpected_regex)
        if unexpected_regex.search(text):
            self.fail(self._mm_msg(msg, "Regex matched: %r" % (unexpected_regex.pattern,)))

    def assertRaises(self, expected_exception, *args, **kwargs):
        msg = kwargs.pop("msg", None)
        ctx = _MMRaisesContext(self, expected_exception, None, msg)
        if not args:
            return ctx
        callable_obj = args[0]
        with ctx:
            callable_obj(*args[1:], **kwargs)
        return ctx

    def assertRaisesRegex(self, expected_exception, expected_regex, *args, **kwargs):
        msg = kwargs.pop("msg", None)
        ctx = _MMRaisesContext(self, expected_exception, expected_regex, msg)
        if not args:
            return ctx
        callable_obj = args[0]
        with ctx:
            callable_obj(*args[1:], **kwargs)
        return ctx

    def assertWarns(self, expected_warning, *args, **kwargs):
        ctx = _MMWarnsContext(self, expected_warning)
        if not args:
            return ctx
        with ctx:
            args[0](*args[1:], **kwargs)
        return ctx

    def assertWarnsRegex(self, expected_warning, expected_regex, *args, **kwargs):
        ctx = _MMWarnsContext(self, expected_warning, expected_regex)
        if not args:
            return ctx
        with ctx:
            args[0](*args[1:], **kwargs)
        return ctx

    def assertLogs(self, logger=None, level=None):
        return _MMLogsContext(self, logger, level, False)

    def assertNoLogs(self, logger=None, level=None):
        return _MMLogsContext(self, logger, level, True)


class _MMAsyncTestCase(_MMTestCase):
    async def asyncSetUp(self):
        pass

    async def asyncTearDown(self):
        pass

    def addAsyncCleanup(self, function, *args, **kwargs):
        self._cleanups.append((function, args, kwargs))

    async def enterAsyncContext(self, cm):
        result = await type(cm).__aenter__(cm)
        self.addAsyncCleanup(type(cm).__aexit__, cm, None, None, None)
        return result


class _MMTestSuite:
    def __init__(self, tests=()):
        self._tests = list(tests)

    def addTest(self, test):
        self._tests.append(test)

    def addTests(self, tests):
        for t in tests:
            self._tests.append(t)


def _mm_ut_main(*args, **kwargs):
    return None


_mm_module_cleanups = []


def _mm_add_module_cleanup(function, *args, **kwargs):
    _mm_module_cleanups.append((function, args, kwargs))


def _mm_enter_module_context(cm):
    result = type(cm).__enter__(cm)
    _mm_add_module_cleanup(type(cm).__exit__, cm, None, None, None)
    return result


class _MMTestLoader:
    def loadTestsFromTestCase(self, cls):
        return _MMTestSuite()

    def loadTestsFromModule(self, *args, **kwargs):
        return _MMTestSuite()


_mm_unittest = _MMNamespace("unittest")
_mm_unittest.TestCase = _MMTestCase
_mm_unittest.IsolatedAsyncioTestCase = _MMAsyncTestCase
_mm_unittest.SkipTest = _MMSkipTest
_mm_unittest.skip = _mm_skip
_mm_unittest.skipIf = _mm_skip_if
_mm_unittest.skipUnless = _mm_skip_unless
_mm_unittest.expectedFailure = _mm_expected_failure
_mm_unittest.main = _mm_ut_main
_mm_unittest.addModuleCleanup = _mm_add_module_cleanup
_mm_unittest.enterModuleContext = _mm_enter_module_context
_mm_unittest.TestSuite = _MMTestSuite
_mm_unittest.TestLoader = _MMTestLoader
_mm_unittest.defaultTestLoader = _MMTestLoader()
_mm_unittest.case = _mm_unittest
_mm_unittest.mock = _MMMissing("unittest.mock")


# ------------------------------------------------------------ test.support

class _MMAlwaysEq:
    def __eq__(self, other):
        return True

    def __ne__(self, other):
        return False

    def __hash__(self):
        return 0


class _MMNeverEq:
    def __eq__(self, other):
        return False

    def __ne__(self, other):
        return True

    def __hash__(self):
        return 1


class _MMLargest:
    def __eq__(self, other):
        return isinstance(other, _MMLargest)

    def __lt__(self, other):
        return False

    def __le__(self, other):
        return isinstance(other, _MMLargest)

    def __gt__(self, other):
        return not isinstance(other, _MMLargest)

    def __ge__(self, other):
        return True

    def __hash__(self):
        return 2


class _MMSmallest:
    def __eq__(self, other):
        return isinstance(other, _MMSmallest)

    def __lt__(self, other):
        return not isinstance(other, _MMSmallest)

    def __le__(self, other):
        return True

    def __gt__(self, other):
        return False

    def __ge__(self, other):
        return isinstance(other, _MMSmallest)

    def __hash__(self):
        return 3


class _MMNullContext:
    def __init__(self, value=None):
        self.value = value

    def __enter__(self):
        return self.value

    def __exit__(self, *exc):
        return False


def _mm_null_context(*args, **kwargs):
    return _MMNullContext()


def _mm_bare_use(args, kwargs):
    return len(args) == 1 and not kwargs and callable(args[0]) and not isinstance(args[0], str)


def _mm_decorator_factory_noop(*args, **kwargs):
    # usable bare (@deco) or called (@deco(...))
    if _mm_bare_use(args, kwargs):
        return args[0]
    return _mm_identity


def _mm_decorator_factory_skip(why):
    def factory(*args, **kwargs):
        if _mm_bare_use(args, kwargs):
            return _mm_skip(why)(args[0])
        return _mm_skip(why)
    return factory


class _MMSwapAttr:
    def __init__(self, obj, attr, new_val):
        self.obj = obj
        self.attr = attr
        self.new_val = new_val

    def __enter__(self):
        self.had = hasattr(self.obj, self.attr)
        if self.had:
            self.old = getattr(self.obj, self.attr)
        setattr(self.obj, self.attr, self.new_val)
        return self.old if self.had else None

    def __exit__(self, *exc):
        if self.had:
            setattr(self.obj, self.attr, self.old)
        else:
            delattr(self.obj, self.attr)
        return False


class _MMSwapItem:
    def __init__(self, obj, item, new_val):
        self.obj = obj
        self.item = item
        self.new_val = new_val

    def __enter__(self):
        self.had = self.item in self.obj
        if self.had:
            self.old = self.obj[self.item]
        self.obj[self.item] = self.new_val
        return self.old if self.had else None

    def __exit__(self, *exc):
        if self.had:
            self.obj[self.item] = self.old
        else:
            del self.obj[self.item]
        return False


class _MMCaptured:
    def __init__(self, name):
        self.name = name

    def __enter__(self):
        import io
        self.old = getattr(_mm_sys, self.name)
        self.buf = io.StringIO()
        setattr(_mm_sys, self.name, self.buf)
        return self.buf

    def __exit__(self, *exc):
        setattr(_mm_sys, self.name, self.old)
        return False


def _mm_captured_stdout():
    return _MMCaptured("stdout")


def _mm_captured_stderr():
    return _MMCaptured("stderr")


def _mm_gc_collect():
    import gc
    gc.collect()


def _mm_run_unittest(*classes):
    return None


def _mm_check_syntax_error(testcase, statement, errtext="", lineno=None, offset=None):
    with testcase.assertRaisesRegex(SyntaxError, errtext) as cm:
        compile(statement, "<test string>", "exec")
    err = cm.exception
    if lineno is not None:
        testcase.assertEqual(err.lineno, lineno)
    if offset is not None:
        testcase.assertEqual(err.offset, offset)


def _mm_sortdict(d):
    items = sorted(d.items())
    return "{" + ", ".join("%r: %r" % (k, v) for k, v in items) + "}"


def _mm_check_impl_detail(**guards):
    return False


def _mm_check_disallow_instantiation(testcase, tp, *args, **kwds):
    qualname = tp.__name__
    msg = "cannot create '" + qualname + "' instances"
    testcase.assertRaisesRegex(TypeError, msg, tp, *args, **kwds)


def _mm_exceeds_recursion_limit():
    return 150000


def _mm_sub_tests(arg_names, arg_values, _do_cleanups=False):
    single_param = False
    if isinstance(arg_names, str):
        arg_names = arg_names.replace(",", " ").split()
        if len(arg_names) == 1:
            single_param = True
    arg_values = tuple(arg_values)

    def decorator(func):
        def wrapper(self, *args, **kwargs):
            for values in arg_values:
                if single_param:
                    values = (values,)
                subtest_kwargs = dict(zip(arg_names, values))
                with self.subTest(**subtest_kwargs):
                    func(self, *args, **kwargs, **subtest_kwargs)
                if _do_cleanups:
                    self.doCleanups()
        wrapper.__name__ = func.__name__
        return wrapper
    return decorator


def _mm_get_attribute(obj, name):
    try:
        return getattr(obj, name)
    except AttributeError:
        raise _MMSkipTest("object %r has no attribute %r" % (obj, name))


class _MMBrokenIter:
    def __init__(self, init_raises=False, next_raises=False, iter_raises=False):
        if init_raises:
            1 / 0
        self.next_raises = next_raises
        self.iter_raises = iter_raises

    def __next__(self):
        if self.next_raises:
            1 / 0

    def __iter__(self):
        if self.iter_raises:
            1 / 0
        return self


def _mm_check_free_after_iterating(*args, **kwargs):
    # needs __del__, which the subset rejects; skipped on both engines
    raise _MMSkipTest("check_free_after_iterating needs __del__ (shim)")


class _MMAdjustIntMaxStrDigits:
    def __init__(self, n):
        self.n = n

    def __enter__(self):
        self.old = _mm_sys.get_int_max_str_digits()
        _mm_sys.set_int_max_str_digits(self.n)

    def __exit__(self, *exc):
        _mm_sys.set_int_max_str_digits(self.old)
        return False


def _mm_adjust_int_max_str_digits(n):
    return _MMAdjustIntMaxStrDigits(n)


def _mm_false(*args, **kwargs):
    return False


class _MMTestFailed(Exception):
    pass


def _mm_needs_subprocess(*args, **kwargs):
    raise _MMSkipTest("needs a subprocess (shim)")


def _mm_run_no_yield_async_fn(async_fn, *args, **kwargs):
    coro = async_fn(*args, **kwargs)
    try:
        coro.send(None)
    except StopIteration as e:
        return e.value
    else:
        raise AssertionError("coroutine did not complete")
    finally:
        coro.close()


def _mm_run_yielding_async_fn(async_fn, *args, **kwargs):
    coro = async_fn(*args, **kwargs)
    try:
        while True:
            try:
                coro.send(None)
            except StopIteration as e:
                return e.value
    finally:
        coro.close()


class _MMAnything:
    # hypothesis strategies: built at decoration time, never drawn from
    def __getattr__(self, attr):
        if attr.startswith("__"):
            raise AttributeError(attr)
        return self

    def __call__(self, *args, **kwargs):
        return self


def _mm_unlink(filename):
    import os
    try:
        os.unlink(filename)
    except (FileNotFoundError, NotADirectoryError):
        pass


class _MMFakePath:
    def __init__(self, path):
        self.path = path

    def __repr__(self):
        return "<FakePath %r>" % (self.path,)

    def __fspath__(self):
        if isinstance(self.path, BaseException):
            raise self.path
        return self.path


def _mm_ignore_warnings(category=None):
    # mm has no warnings module and emits no warnings, so ignoring them is a
    # no-op there
    def deco(f):
        def wrapper(*args, **kwargs):
            try:
                import warnings
            except ImportError:
                return f(*args, **kwargs)
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", category=category)
                return f(*args, **kwargs)
        wrapper.__name__ = f.__name__
        return wrapper
    return deco


class _MMWarningsRecorder:
    def __init__(self, log):
        self._warnings = log
        self._last = 0

    def __getattr__(self, attr):
        if len(self._warnings) > self._last:
            return getattr(self._warnings[-1], attr)
        if attr in ("message", "category", "filename", "lineno", "file", "line", "source"):
            return None
        raise AttributeError(attr)

    @property
    def warnings(self):
        return self._warnings[self._last:]

    def reset(self):
        self._last = len(self._warnings)


class _MMCheckWarnings:
    def __init__(self, filters, quiet):
        self.filters = filters
        self.quiet = quiet

    def __enter__(self):
        import warnings
        self.cm = warnings.catch_warnings(record=True)
        self.log = self.cm.__enter__()
        warnings.simplefilter("always")
        return _MMWarningsRecorder(self.log)

    def __exit__(self, exc_type, exc_value, tb):
        self.cm.__exit__(exc_type, exc_value, tb)
        if exc_type is not None:
            return False
        import re
        reraise = list(self.log)
        missing = []
        for msg, cat in self.filters:
            seen = False
            for w in reraise[:]:
                if re.match(msg, str(w.message), re.I) and issubclass(w.category, cat):
                    seen = True
                    reraise.remove(w)
            if not seen and not self.quiet:
                missing.append((msg, cat.__name__))
        if reraise:
            raise AssertionError("unhandled warning %s" % reraise[0])
        if missing:
            raise AssertionError("filter (%r, %s) did not catch any warning" % missing[0])
        return False


def _mm_check_warnings(*filters, **kwargs):
    quiet = kwargs.get("quiet")
    if not filters:
        filters = (("", Warning),)
        if quiet is None:
            quiet = True
    return _MMCheckWarnings(filters, quiet)


def _mm_check_syntax_warning(testcase, statement, errtext="", lineno=1, offset=None):
    import warnings
    with warnings.catch_warnings(record=True) as warns:
        warnings.simplefilter("always", SyntaxWarning)
        compile(statement, "<testcase>", "exec")
    testcase.assertEqual(len(warns), 1, warns)
    warn = warns[0]
    testcase.assertTrue(issubclass(warn.category, SyntaxWarning))
    if errtext:
        testcase.assertRegex(str(warn.message), errtext)
    testcase.assertEqual(warn.filename, "<testcase>")
    testcase.assertIsNotNone(warn.lineno)
    if lineno is not None:
        testcase.assertEqual(warn.lineno, lineno)
    with warnings.catch_warnings(record=True) as warns:
        warnings.simplefilter("error", SyntaxWarning)
        _mm_check_syntax_error(testcase, statement, errtext, lineno=lineno, offset=offset)
    testcase.assertEqual(warns, [])


# Modules a test may ask for by name (import_helper.import_module,
# import_fresh_module). The harness fills this table with plain import
# statements it generates for every string literal passed to those helpers,
# because the subset has no __import__.
_mm_known_modules = {}


def _mm_import_module(name, *args, **kwargs):
    if name in _mm_known_modules:
        return _mm_known_modules[name]
    raise _MMSkipTest("No module named " + repr(name))


def _mm_import_fresh_module(name, fresh=(), blocked=(), *args, **kwargs):
    # C accelerator variants (fresh=['_json'] etc.) are not tested: the shim
    # returns None for them, which the tests treat as "C module missing".
    for f in fresh:
        if f.startswith("_"):
            return None
    if name in _mm_known_modules:
        return _mm_known_modules[name]
    return None


_mm_support = _MMNamespace("test.support")
_mm_support.verbose = 0
_mm_support.cpython_only = _mm_skip("cpython_only (shim)")
_mm_support.impl_detail = _mm_decorator_factory_skip("impl_detail (shim)")
_mm_support.check_impl_detail = _mm_check_impl_detail
_mm_support.refcount_test = _mm_skip("refcount_test (shim)")
_mm_support.bigmemtest = _mm_decorator_factory_skip("bigmemtest (shim)")
_mm_support.bigaddrspacetest = _mm_skip("bigaddrspacetest (shim)")
_mm_support.requires_resource = _mm_decorator_factory_skip("requires_resource (shim)")
_mm_support.requires_subprocess = _mm_decorator_factory_skip("requires_subprocess (shim)")
_mm_support.requires_fork = _mm_decorator_factory_skip("requires_fork (shim)")
_mm_support.requires_working_socket = _mm_decorator_factory_skip("requires_working_socket (shim)")
_mm_support.requires_limited_api = _mm_skip("requires_limited_api (shim)")
_mm_support.requires_IEEE_754 = _mm_identity
_mm_support.requires_docstrings = _mm_identity
_mm_support.requires_debug_ranges = _mm_decorator_factory_noop
_mm_support.no_tracing = _mm_identity
_mm_support.skip_if_pgo_task = _mm_identity
_mm_support.skip_emscripten_stack_overflow = _mm_decorator_factory_noop
_mm_support.skip_wasi_stack_overflow = _mm_decorator_factory_noop
_mm_support.skip_if_sanitizer = _mm_decorator_factory_noop
_mm_support.skip_on_s390x = _mm_identity
_mm_support.thread_unsafe = _mm_decorator_factory_noop
_mm_support.run_with_locale = _mm_decorator_factory_noop
_mm_support.run_with_locales = _mm_decorator_factory_noop
_mm_support.run_with_tz = _mm_decorator_factory_skip("run_with_tz (shim)")
_mm_support.requires_gil_enabled = _mm_decorator_factory_noop
_mm_support.anticipate_failure = _mm_decorator_factory_noop
_mm_support.is_emscripten = False
_mm_support.is_wasi = False
_mm_support.is_android = False
_mm_support.is_apple_mobile = False
_mm_support.is_apple = False
_mm_support.Py_DEBUG = False
_mm_support.Py_GIL_DISABLED = False
_mm_support.MISSING_C_DOCSTRINGS = False
_mm_support.HAVE_DOCSTRINGS = True
_mm_support.MAX_Py_ssize_t = _mm_sys.maxsize
_mm_support.SHORT_TIMEOUT = 30.0
_mm_support.LOOPBACK_TIMEOUT = 10.0
_mm_support.LONG_TIMEOUT = 300.0
_mm_support.INTERNET_TIMEOUT = 60.0
_mm_support.ALWAYS_EQ = _MMAlwaysEq()
_mm_support.NEVER_EQ = _MMNeverEq()
_mm_support.LARGEST = _MMLargest()
_mm_support.SMALLEST = _MMSmallest()
_mm_support.swap_attr = _MMSwapAttr
_mm_support.swap_item = _MMSwapItem
_mm_support.captured_stdout = _mm_captured_stdout
_mm_support.captured_stderr = _mm_captured_stderr
_mm_support.gc_collect = _mm_gc_collect
_mm_support.disable_gc = _mm_null_context
_mm_support.infinite_recursion = _mm_null_context
_mm_support.exceeds_recursion_limit = _mm_exceeds_recursion_limit
_mm_support.run_unittest = _mm_run_unittest
_mm_support.check_syntax_error = _mm_check_syntax_error
_mm_support.sortdict = _mm_sortdict
_mm_support.check_disallow_instantiation = _mm_check_disallow_instantiation
_mm_support.subTests = _mm_sub_tests
_mm_support.get_attribute = _mm_get_attribute
_mm_support.BrokenIter = _MMBrokenIter
_mm_support.check_free_after_iterating = _mm_check_free_after_iterating
_mm_support.adjust_int_max_str_digits = _mm_adjust_int_max_str_digits
_mm_support.run_with_limited_c_stack = _mm_decorator_factory_noop
_mm_support.skip_if_huge_c_stack = _mm_decorator_factory_noop
_mm_support.skip_if_unlimited_stack_size = _mm_identity
_mm_support.nomemtest = _mm_skip("nomemtest needs _testcapi (shim)")
_mm_support.force_not_colorized = _mm_identity
_mm_support.force_not_colorized_test_class = _mm_identity
_mm_support.requires_mac_ver = _mm_decorator_factory_noop
_mm_support.linked_to_musl = _mm_false
_mm_support.is_resource_enabled = _mm_false
_mm_support.HAVE_PY_DOCSTRINGS = True
_mm_support.captured_output = _MMCaptured
_mm_support.TestFailed = _MMTestFailed
_mm_support.import_module = _mm_import_module
_mm_support.import_fresh_module = _mm_import_fresh_module

_mm_import_helper = _MMNamespace("test.support.import_helper")
_mm_import_helper.import_module = _mm_import_module
_mm_import_helper.import_fresh_module = _mm_import_fresh_module
_mm_support.import_helper = _mm_import_helper
_mm_support._1M = 1024 * 1024
_mm_support._1G = 1024 * _mm_support._1M
_mm_support._2G = 2 * _mm_support._1G
_mm_support._4G = 4 * _mm_support._1G
_mm_support.run_yielding_async_fn = _mm_run_yielding_async_fn
_mm_support.run_no_yield_async_fn = _mm_run_no_yield_async_fn
_mm_os_helper = _MMNamespace("test.support.os_helper")
_mm_os_helper.FakePath = _MMFakePath
_mm_os_helper.TESTFN = "@test_mm_tmp"
_mm_os_helper.TESTFN_ASCII = "@test_mm_tmp"
_mm_os_helper.unlink = _mm_unlink
_mm_os_helper.skip_unless_symlink = _mm_skip("symlinks (shim)")
_mm_support.os_helper = _mm_os_helper
_mm_warnings_helper = _MMNamespace("test.support.warnings_helper")
_mm_warnings_helper.ignore_warnings = _mm_ignore_warnings
_mm_warnings_helper.check_warnings = _mm_check_warnings
_mm_warnings_helper.check_syntax_warning = _mm_check_syntax_warning
_mm_support.warnings_helper = _mm_warnings_helper
_mm_script_helper = _MMNamespace("test.support.script_helper")
_mm_script_helper.assert_python_ok = _mm_needs_subprocess
_mm_script_helper.assert_python_failure = _mm_needs_subprocess
_mm_script_helper.spawn_python = _mm_needs_subprocess
_mm_script_helper.run_python_until_end = _mm_needs_subprocess
_mm_script_helper.interpreter_requires_environment = _mm_false
_mm_support.script_helper = _mm_script_helper
_mm_threading_helper = _MMNamespace("test.support.threading_helper")
_mm_threading_helper.requires_working_threading = _mm_decorator_factory_skip("threads (shim)")
_mm_threading_helper.reap_threads = _mm_identity
_mm_support.threading_helper = _mm_threading_helper
_mm_hypothesis = _MMNamespace("hypothesis")
_mm_hypothesis.given = _mm_decorator_factory_skip("hypothesis (shim)")
_mm_hypothesis.example = _mm_decorator_factory_noop
_mm_hypothesis.settings = _mm_decorator_factory_noop
_mm_hypothesis.assume = _mm_false
_mm_hypothesis.strategies = _MMAnything()
_mm_hypothesis_helper = _MMNamespace("test.support.hypothesis_helper")
_mm_hypothesis_helper.hypothesis = _mm_hypothesis
_mm_support.hypothesis_helper = _mm_hypothesis_helper
_mm_support.numbers = _MMNamespace("test.support.numbers")


# ---------------------------------------------------------------- runner

def _mm_last_line(e):
    # every traceback line number, outermost first; the harness maps them
    # back to source origins
    tb = getattr(e, "__traceback__", None)
    out = []
    while tb is not None:
        out.append(str(tb.tb_lineno))
        tb = tb.tb_next
    return ",".join(out[-12:])


def _mm_report(test_id, status, e=None):
    if e is None:
        _mm_emit("@@MMT", test_id, status, "", "", "")
    else:
        _mm_emit("@@MMT", test_id, status, type(e).__name__, _mm_last_line(e), e)


def _mm_is_coroutine(obj):
    return obj is not None and type(obj).__name__ == "coroutine"


def _mm_call_test(test, method):
    if isinstance(test, _MMAsyncTestCase):
        import asyncio

        async def wrapper():
            await test.asyncSetUp()
            try:
                res = method()
                if _mm_is_coroutine(res):
                    await res
            finally:
                await test.asyncTearDown()
                while test._cleanups:
                    function, args, kwargs = test._cleanups.pop()
                    res = function(*args, **kwargs)
                    if _mm_is_coroutine(res):
                        await res

        test.setUp()
        try:
            asyncio.run(wrapper())
        finally:
            test.tearDown()
        return
    test.setUp()
    try:
        res = method()
        if _mm_is_coroutine(res):
            res.close()
    finally:
        test.tearDown()


def _mm_run_one(cls, name, cname):
    test_id = cname + "." + name
    _mm_emit("@@MMSTART", test_id)
    try:
        method_fn = getattr(cls, name)
    except BaseException as e:
        _mm_report(test_id, "error", e)
        return
    if getattr(method_fn, "__unittest_skip__", False):
        _mm_report(test_id, "skip")
        return
    expecting = getattr(method_fn, "__unittest_expecting_failure__", False)
    try:
        test = cls(name)
        method = getattr(test, name)
    except BaseException as e:
        _mm_report(test_id, "error", e)
        return
    outcome = None
    exc = None
    try:
        _mm_call_test(test, method)
    except _MMSkipTest as e:
        outcome = "skip"
    except AssertionError as e:
        outcome = "fail"
        exc = e
    except Exception as e:
        outcome = "error"
        exc = e
    try:
        test.doCleanups()
    except Exception as e:
        if outcome is None:
            outcome = "error"
            exc = e
    if outcome is None and test._mm_subfailure is not None:
        exc = test._mm_subfailure
        outcome = "fail" if isinstance(exc, AssertionError) else "error"
    if outcome == "skip":
        _mm_report(test_id, "skip")
    elif expecting:
        if outcome is None:
            _mm_report(test_id, "xpass")
        else:
            _mm_report(test_id, "xfail")
    elif outcome is None:
        _mm_report(test_id, "pass")
    else:
        _mm_report(test_id, outcome, exc)


def _mm_run(plan, classes, module_setup, module_teardown):
    # plan: list of (class name, [method names]); classes: name -> thunk.
    try:
        if module_setup is not None:
            module_setup()
    except BaseException as e:
        for cname, methods in plan:
            for m in methods:
                _mm_emit("@@MMSTART", cname + "." + m)
                _mm_report(cname + "." + m, "skip" if isinstance(e, _MMSkipTest) else "error", e)
        return
    for cname, methods in plan:
        try:
            cls = classes[cname]()
        except BaseException as e:
            for m in methods:
                _mm_emit("@@MMSTART", cname + "." + m)
                _mm_report(cname + "." + m, "error", e)
            continue
        if getattr(cls, "__unittest_skip__", False):
            for m in methods:
                _mm_emit("@@MMSTART", cname + "." + m)
                _mm_report(cname + "." + m, "skip")
            continue
        try:
            cls.setUpClass()
        except BaseException as e:
            for m in methods:
                _mm_emit("@@MMSTART", cname + "." + m)
                _mm_report(cname + "." + m, "skip" if isinstance(e, _MMSkipTest) else "error", e)
            try:
                cls.doClassCleanups()
            except BaseException:
                pass
            continue
        for m in methods:
            _mm_run_one(cls, m, cname)
        try:
            cls.tearDownClass()
            cls.doClassCleanups()
        except BaseException as e:
            _mm_emit("@@MMCLASSTEARDOWN", cname, type(e).__name__, e)
    if module_teardown is not None:
        try:
            module_teardown()
        except BaseException:
            pass
    while _mm_module_cleanups:
        function, args, kwargs = _mm_module_cleanups.pop()
        try:
            function(*args, **kwargs)
        except BaseException:
            pass
    _mm_emit("@@MMDONE")


def _mm_discover(g, min_line):
    # CPython only: list TestCase subclasses bound to module globals and
    # their test* methods the way unittest's loader does (dir(), callable,
    # sorted). A class bound under several names is listed once, under its
    # own name when that is one of them.
    for name in sorted(g):
        obj = g[name]
        if not isinstance(obj, type) or not issubclass(obj, _MMTestCase):
            continue
        if obj is _MMTestCase or obj is _MMAsyncTestCase:
            continue
        if obj.__name__ != name:
            if g.get(obj.__name__) is obj:
                continue
            if any(g[n] is obj for n in sorted(g) if n < name):
                continue
        line = getattr(obj, "__firstlineno__", 0)
        if line < min_line:
            continue
        methods = [m for m in dir(obj) if m.startswith("test") and callable(getattr(obj, m))]
        _mm_emit("@@MMCLASS", name, ",".join(sorted(methods)))
    _mm_emit("@@MMDONE")
