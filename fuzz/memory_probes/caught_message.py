try:
    raise ValueError('a' * (M // 3))
except ValueError as e:
    x = str(e)
