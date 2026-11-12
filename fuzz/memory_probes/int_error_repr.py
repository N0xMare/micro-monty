try:
    int('\x00' * (M // 3))
except ValueError:
    pass
