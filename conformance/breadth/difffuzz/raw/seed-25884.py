# difffuzz divergence, seed 25884
# CPython: exit 0, last stderr line: 
# mm:      exit 0, last stderr line: 
# first difference (stdout line 1, 0 = exit):
#   CPython: ['a,b,,ct\tr\u0b98t\tr']
#   mm:      ['a,b,,ct\tr஘t\tr']
# signature: CPython: ['a,b,,ct\tr\u#b#t\tr'] | mm: ['a,b,,ct\tr஘t\tr']
import math
try:
    data = ('a,b,,c' + chr(2968).replace('123'.partition(' ')[1], 'tab\there'[::3])).splitlines()
    print(sorted(data, key=len, reverse=False))
except Exception as exc:
    print("EXC", type(exc).__name__, exc)
