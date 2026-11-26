import importlib
import sys

from base64 import b64decode

print(b64decode('TQ=='))
assert 'binascii' in sys.modules
assert '_mm' not in sys.modules
assert '_mm' not in list(sys.modules)
assert '_mm' not in sys.builtin_module_names
assert sys.modules.get('_mm') is None
assert len(sys.modules) == len(list(sys.modules))
try:
    importlib.import_module('_mm')
except ModuleNotFoundError as e:
    print(str(e), e.name)
else:
    raise AssertionError('_mm imported')

import binascii
import traceback

try:
    binascii.a2b_base64(None)
except TypeError as e:
    print([frame.name for frame in traceback.extract_tb(e.__traceback__)])
else:
    raise AssertionError('None decoded as base64')
