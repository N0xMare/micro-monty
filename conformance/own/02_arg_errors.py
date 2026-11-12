def f(a, b, c=3):
    return a

def kwonly(*, x):
    return x

tests = [
    lambda: f(),
    lambda: f(1),
    lambda: f(1, 2, 3, 4),
    lambda: f(1, 2, d=4),
    lambda: f(1, 2, a=4),
    lambda: kwonly(),
    lambda: kwonly(1),
    lambda: len(),
    lambda: len(1, 2),
    lambda: f(1, 2, cc=1),
]
for t in tests:
    try:
        t()
    except TypeError as e:
        print("TypeError:", e)
