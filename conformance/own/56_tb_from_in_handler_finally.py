try:
    1 / 0
except ZeroDivisionError as e:
    raise ValueError("bad") from e
finally:
    print("finally")
