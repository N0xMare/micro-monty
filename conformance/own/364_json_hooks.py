# json.loads hooks: parse_float/parse_int/parse_constant/object_hook/
# object_pairs_hook, called children-first in document order as CPython's
# scanner calls them; object_pairs_hook wins over object_hook; Decimal.
import json
from decimal import Decimal
from collections import OrderedDict
log = []
def tag(n):
    def f(x):
        log.append((n, x))
        return (n, x)
    return f
doc = '{"a": 1, "b": [2.5, {"c": -0, "d": NaN}, {}], "e": {"f": 1e3, "g": -Infinity, "f": Infinity}, "h": [], "i": "s", "j": null, "k": true}'
r = json.loads(doc, parse_float=tag('pf'), parse_int=tag('pi'), parse_constant=tag('pc'), object_hook=tag('oh'))
print(r)
for x in log: print(x)
log.clear()
r = json.loads(doc, object_pairs_hook=tag('ph'), object_hook=tag('oh'))
print(r)
for x in log: print(x)
print(json.loads('[1.10, 2, {"x": 0.30}]', parse_float=Decimal))
print(json.loads('1.5', parse_float=Decimal), json.loads('7', parse_int=float), json.loads('{}', object_hook=len))
print(json.loads('{"a": 1, "a": 2}', object_pairs_hook=OrderedDict))
print(json.loads('{"a": 1, "b": {"c": 2}}', object_hook=lambda d: sorted(d)))
print(json.loads('[NaN, 1E5, 12345678901234567890123]', parse_int=str))
try:
    json.loads('[1, 2, x]', parse_int=int)
except json.JSONDecodeError as e:
    print('err', e)
def bad(x): raise ValueError('nope ' + x)
try:
    json.loads('[1, 2]', parse_int=bad)
except ValueError as e:
    print('err', e)
raw = json.dumps({"price": "x", "n": [1.1, 2.2]})
print(json.loads(raw, parse_float=Decimal))
