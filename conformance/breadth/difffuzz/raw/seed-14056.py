# difffuzz divergence, seed 14056
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: {'123mississippix\nymississippimississippi': 39, 'z': 1} ['\u0cfd', 717897987691852588770249, 2]
#   mm:      {'123mississippix\nymississippimississippi': 39, 'z': 1} ['೽', 717897987691852588770249, 2]
# signature: CPython: {'#mississippix\nymississippimississippi': #, 'z': #} ['\u#cfd', #, #] | mm: {'#mississippix\nymississippimississippi': #, 'z': #} ['೽', #, #]
import math
try:
    v2 = 3 ** 50
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
try:
    print({**{k15: len(k15) for k15 in list({'mississippi'.join(['123', 'x\ny', 'mississippi']): math.isqrt(abs(255) % 30)})}, "z": 1}, [*(chr(3325).title(), ), *list({'\u00df'.center(6, chr(45)).rjust(8, chr(45)): v2, ('caf\u00e9' + 'hello').join('  pad  '.split()): round(1.5)}.values())])
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
