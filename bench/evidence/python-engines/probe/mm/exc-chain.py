try:
    try:
        1/0
    except ZeroDivisionError as e:
        raise ValueError("v") from e
except ValueError as e:
    r = type(e.__cause__).__name__
print(r)
