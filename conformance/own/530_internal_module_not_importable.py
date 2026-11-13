# `_mm`, the bundled modules' engine helpers, is not a module user code can
# import, by any route, and no bundled module holds it as an attribute.
import sys

attempts = [
    'import _mm',
    'from _mm import clock',
    'import _mm.x',
    "__import__('_mm')",
    "__import__('_mm', fromlist=['clock'])",
]
for src in attempts:
    try:
        exec_ok = False
        if src == 'import _mm':
            import _mm
        elif src == 'from _mm import clock':
            from _mm import clock
        elif src == 'import _mm.x':
            import _mm.x
        elif src == "__import__('_mm')":
            __import__('_mm')
        else:
            __import__('_mm', fromlist=['clock'])
        exec_ok = True
    except ModuleNotFoundError as e:
        print(src, '->', type(e).__name__, e, e.name)
    print(src, 'imported' if exec_ok else 'refused')

print('_mm' in sys.builtin_module_names)

import collections
import contextvars
import copy
import dataclasses
import gc
import os
import random
import time
import types
import warnings
import zoneinfo

modules = [collections, contextvars, copy, dataclasses, gc, os, random, time,
           types, warnings, zoneinfo]
print([m.__name__ for m in modules if '_mm' in dir(m)])

# The modules still work through the names they took from `_mm`.
Point = collections.namedtuple('Point', 'x y')
print(Point(1, 2), copy.copy(collections.OrderedDict(a=1)))


@dataclasses.dataclass
class C:
    a: int
    b: str = 'b'


print(C(1), types.FunctionType is type(lambda: 0))
v = contextvars.ContextVar('v', default=0)
print(contextvars.copy_context().run(lambda: (v.set(5), v.get())[1]), v.get())
print(gc.collect() >= 0, isinstance(time.time(), float), len(os.urandom(4)))
print(random.Random(3).random() == random.Random(3).random())
with warnings.catch_warnings(record=True) as w:
    warnings.simplefilter('always')
    warnings.warn('careful')
print([str(x.message) for x in w])
print(zoneinfo.ZoneInfo('UTC').key)
