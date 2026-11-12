def handler():
    try:
        raise ValueError("first")
    except ValueError:
        try:
            raise TypeError("second")
        except TypeError:
            pass
        raise

try:
    handler()
except ValueError as e:
    print("bare raise re-raised", e)

try:
    raise
except RuntimeError as e:
    print(e)

e = None
try:
    raise KeyError("x")
except KeyError as e:
    pass
try:
    print(e)
except NameError as n:
    print("NameError:", n)
