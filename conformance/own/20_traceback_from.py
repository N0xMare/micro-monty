def parse(s):
    return int(s)

def load(s):
    try:
        return parse(s)
    except ValueError as e:
        raise RuntimeError(f"cannot load {s!r}") from e

load("x1")
