pairs = [
    (ZeroDivisionError, ArithmeticError), (OverflowError, ArithmeticError),
    (IndexError, LookupError), (KeyError, LookupError), (UnboundLocalError, NameError),
    (RecursionError, RuntimeError), (NotImplementedError, RuntimeError),
    (Exception, BaseException), (KeyboardInterrupt, Exception), (StopIteration, Exception),
    (MemoryError, Exception), (AssertionError, Exception), (TypeError, ValueError),
]
for sub, sup in pairs:
    e = sub("m")
    print(sub.__name__, sup.__name__, isinstance(e, sup), isinstance(e, BaseException))
print(isinstance(ValueError(), (TypeError, ValueError)), isinstance(1, (int, str)))
e = KeyError("key")
print(repr(e), str(e), e.args)
e = ValueError()
print(repr(e), str(e), e.args)
e = ValueError(1, 2)
print(repr(e), str(e))
print(repr(Exception("a")), type(Exception("a")), ZeroDivisionError)
try:
    raise ValueError
except ValueError as v:
    print(repr(v))
try:
    raise 5
except TypeError as t:
    print(t)
try:
    try:
        pass
    except 5:
        pass
    raise KeyError(1)
except TypeError as t:
    print("unreachable", t)
except KeyError as k:
    print("KeyError", k)
