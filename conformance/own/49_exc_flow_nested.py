def f():
    for i in range(3):
        try:
            try:
                if i == 0:
                    raise ValueError(i)
                if i == 1:
                    continue
                return i
            except ValueError as e:
                print("caught", e)
                continue
            finally:
                print("inner fin", i)
        finally:
            print("outer fin", i)
    return "end"
print(f())

def g():
    try:
        raise KeyError("a")
    except KeyError as e:
        try:
            raise ValueError("b")
        except ValueError as e2:
            print(repr(e2.__context__))
        print(repr(e))
    try:
        e
    except NameError as ne:
        print("e unbound:", ne)
g()

def h():
    try:
        return 1
    finally:
        try:
            raise ValueError
        except ValueError:
            print("handled inside finally")
print(h())

def k():
    x = 0
    while True:
        x += 1
        try:
            if x > 3:
                break
        finally:
            if x == 2:
                print("fin at 2")
    return x
print(k())

def gen_exc():
    try:
        try:
            1/0
        finally:
            print("fin before propagate")
    except ZeroDivisionError as e:
        print("outer caught", type(e).__name__)
gen_exc()

try:
    try:
        raise ValueError("one")
    finally:
        print("cleanup")
except ValueError as e:
    print(e)

def nested_ctx():
    try:
        raise A
    except NameError:
        try:
            raise KeyError(1)
        except KeyError as e:
            return e
r = nested_ctx()
print(repr(r), repr(r.__context__))
e1 = ValueError("x")
print(e1.__context__, e1.__cause__, e1.__traceback__ is None)
print(str(KeyError("a")), str(KeyError("a", "b")), str(KeyError()), repr(KeyError(1)))
print([str(x) for x in [IndexError("list index out of range"), ValueError("v"), Exception()]])
