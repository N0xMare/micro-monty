# difffuzz divergence, seed 2154
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ZeroDivisionError division by zero
#   mm:      EXC MemoryError 
# signature: CPython: EXC ZeroDivisionError division by zero | mm: EXC MemoryError 
import math
try:
    def f1(p0, p1, *rest, kw=3):
        for i2 in range(-2, 7):
            def f4(p0, p1, p2):
                return ('a'.replace(('{}-{!r}-{:>4}'.format(-3, '-42', 0) * (min(p1) % 4)), '') + 'mississippi'.zfill(5).replace(repr(('mississippi' + 'hello')), 'hello'.replace('tab\there', 'a').title()))
    print('f1(divmod((2 ** 64 ** (sum(([] + [10, 255, -1])) % 7)), sum([]))[1], abs([a * b for a, b in zip([i + v for i, v in enumerate([10, 42, 7], 1)], list(range(-3, 11, 2)))][min(([10] * 2))]).bit_length())', f1(divmod((2 ** 64 ** (sum(([] + [10, 255, -1])) % 7)), sum([]))[1], abs([a * b for a, b in zip([i + v for i, v in enumerate([10, 42, 7], 1)], list(range(-3, 11, 2)))][min(([10] * 2))]).bit_length()))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
