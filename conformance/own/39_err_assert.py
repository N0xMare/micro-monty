def check(x):
    assert x > 0, f"expected positive, got {x}"
    return x

check(5)
check(-2)
