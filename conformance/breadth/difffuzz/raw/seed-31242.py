# difffuzz divergence, seed 31242
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: EXC ValueError invalid literal for int() with base 10: "6-'\\u2efa'--1180591620717411303424"
#   mm:      EXC ValueError invalid literal for int() with base 10: "6-'⻺'--1180591620717411303424"
# signature: CPython: EXC ValueError invalid literal for int() with base #: "#-'\\u#efa'--#" | mm: EXC ValueError invalid literal for int() with base #: "#-'⻺'--#"
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def make14(start):
        total = start
        def step(by=1):
            return total
    s = make14(int('{}-{!r}-{:>4}'.format(sum(lst0), chr(12026), -(2 ** 70)).center(6, chr(45))))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
