# Wildcard imports at module level: public names only, `__all__` when the
# module has one, bundled modules and submodules (docs/spec.md §4).
from math import *
print(pi, floor(2.5), sqrt(16.0))
from string import *
print(ascii_lowercase[:5], digits)
from os.path import *
print(join('a', 'b'), basename('/x/y.txt'))
from collections import *
print(OrderedDict, deque([1]))
try:
    print(_chain)
except NameError as e:
    print('NameError', e)
from json import *
print(dumps([1]), JSONDecodeError.__name__)
try:
    print(__all__)
except NameError as e:
    print('NameError', e)
