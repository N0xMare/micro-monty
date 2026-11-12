# `from m import *`: __all__ when the module has one, else the public names.
from math import *
print(pi, sqrt(16), floor(2.5))
from collections import *
print(deque([1]), Counter('aab'), OrderedDict)
from string import *
print(ascii_lowercase, capwords('a b'))
try:
    _KEYWORDS
except NameError as e:
    print('NameError', e)


def f():
    return e_val


e_val = 3
from enum import *
print(f(), Enum, auto)
from collections.abc import *
print(Mapping)
