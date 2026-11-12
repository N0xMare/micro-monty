# difffuzz divergence, seed 17762
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['\u05c8']3.5EXC IndexError list index out of range
#   mm:      ['׈']3.5EXC IndexError list index out of range
# signature: CPython: ['\u#c#']#.#EXC IndexError list index out of range | mm: ['׈']#.#EXC IndexError list index out of range
import math
try:
    print(chr(1480).split(), end='3.5')
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    if (int(('123' * (7 % 4)).split(',')[2], 2) < divmod(sum(list(range(-3, 4, -2))), 1000)[1] <= ((int('  pad  ', 10) ** (round(1e-05) % 7)) >> (divmod((n7 >> (42 % 9)), len('3.5'))[0] % 9))):
        def make23(start):
            total = start
            def step(by=1):
                return total
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
