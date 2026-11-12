def a():
    try:
        1 / 0
    except ZeroDivisionError:
        raise ValueError("while handling")

try:
    a()
except ValueError as e:
    print(repr(e), repr(e.__context__), e.__cause__, e.__suppress_context__)

def b():
    try:
        {}["x"]
    except KeyError as err:
        raise RuntimeError("explicit") from err

try:
    b()
except RuntimeError as e:
    print(repr(e.__cause__), e.__suppress_context__)

def c():
    try:
        1 / 0
    except ZeroDivisionError:
        raise TypeError("no context") from None

try:
    c()
except TypeError as e:
    print(e.__cause__, repr(e.__context__), e.__suppress_context__)

try:
    try:
        raise ValueError(1)
    except ValueError:
        raise
except ValueError as e:
    print("reraised", e)
a()
