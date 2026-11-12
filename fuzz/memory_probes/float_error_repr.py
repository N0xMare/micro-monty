try:
    float('\x00' * (M // 8))
except ValueError:
    pass
