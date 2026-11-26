# A float subclass that overrides `__float__`: math's functions, `divmod`,
# `math.fsum` and one-argument `complex()` take the instance's own value,
# as CPython's `PyFloat_AsDouble` does; `float()` and two-argument
# `complex()` call `__float__` (`PyNumber_Float`). An int subclass's
# `__float__` is called.
import math


class F(float):
    def __float__(self):
        return 2.5


o = F(1.25)
assert math.sqrt(o) == math.sqrt(1.25) and math.floor(o) == 1
assert divmod(o, 2) == (0.0, 1.25) and divmod(3, o) == (2.0, 0.5)
assert math.fsum([o, o]) == 2.5 and math.isclose(o, 1.25)
assert math.fmod(o, 1) == 0.25 and math.modf(o) == (0.25, 1.0)
assert math.copysign(o, -1) == -1.25 and math.ldexp(o, 1) == 2.5
assert math.frexp(o) == (0.625, 1) and math.dist([o], [0]) == 1.25
assert complex(o) == 1.25 and complex(o, o) == 2.5 + 2.5j
assert float(o) == 2.5 and o + 1 == 2.25 and round(o, 1) == 1.2


class H(int):
    def __float__(self):
        return 9.5


h = H(3)
assert math.sqrt(h) == math.sqrt(9.5) and math.fsum([h]) == 9.5
assert complex(h) == 9.5 and divmod(h, 2) == (1, 1)
