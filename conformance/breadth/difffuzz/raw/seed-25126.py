# difffuzz divergence, seed 25126
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 2, 0 = exit):
#   CPython: f4(42, '{}-{!r}-{:>4}'.format(math.ceil(abs(pow(2, 22, 2 ** 61 - 1)) % 30), (('hello' * (42 % 4)) + chr(11897)), ('hello'.count('caf\u00e9') ^ round(-0.0))).rstrip(), (int(((-3 / -3) ** 2)) if all(list(map(lambda q: q * 2 + 1, [a * b for a, b in zip(lst0, lst0)]))) else list(range(4, -1, 2))[len(dic
#   mm:      f4(42, '{}-{!r}-{:>4}'.format(math.ceil(abs(pow(2, 22, 2 ** 61 - 1)) % 30), (('hello' * (42 % 4)) + chr(11897)), ('hello'.count('caf\u00e9') ^ round(-0.0))).rstrip(), (int(((-3 / -3) ** 2)) if all(list(map(lambda q: q * 2 + 1, [a * b for a, b in zip(lst0, lst0)]))) else list(range(4, -1, 2))[len(dic
# signature: CPython: f#(#, '{}-{!r}-{:>#}'.format(math.ceil(abs(pow(#, #, # ** # - #)) % #), (('hello' * (# % #)) + chr(# | mm: f#(#, '{}-{!r}-{:>#}'.format(math.ceil(abs(pow(#, #, # ** # - #)) % #), (('hello' * (# % #)) + chr(#
import math
try:
    lst0 = [3, 1, 2]
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    def f4(p0, p1, p2):
        try:
            print(((abs(sum(lst0)).bit_length() if True else int(repr('123'), 16)) if False else ~(lst0 + lst0)[min(lst0)]) ** -1)
        except ArithmeticError as e:
            print("caught", type(e).__name__, e, e.args)
        return '{}-{!r}-{:>4}'.format(42, p1, p2).zfill(4).replace(p1, '\u00df'.ljust(10, chr(42))[0:]).lower()
    print("f4(42, '{}-{!r}-{:>4}'.format(math.ceil(abs(pow(2, 22, 2 ** 61 - 1)) % 30), (('hello' * (42 % 4)) + chr(11897)), ('hello'.count('caf\\u00e9') ^ round(-0.0))).rstrip(), (int(((-3 / -3) ** 2)) if all(list(map(lambda q: q * 2 + 1, [a * b for a, b in zip(lst0, lst0)]))) else list(range(4, -1, 2))[len(dict(sorted({'k0': 2, 'k1': 2}.items(), key=lambda kv: kv[1])))]))", f4(42, '{}-{!r}-{:>4}'.format(math.ceil(abs(pow(2, 22, 2 ** 61 - 1)) % 30), (('hello' * (42 % 4)) + chr(11897)), ('hello'.count('caf\u00e9') ^ round(-0.0))).rstrip(), (int(((-3 / -3) ** 2)) if all(list(map(lambda q: q * 2 + 1, [a * b for a, b in zip(lst0, lst0)]))) else list(range(4, -1, 2))[len(dict(sorted({'k0': 2, 'k1': 2}.items(), key=lambda kv: kv[1])))])))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
