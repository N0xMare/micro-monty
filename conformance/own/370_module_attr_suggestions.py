# A missing module attribute gets CPython 3.14's "Did you mean" in the
# report (not in str(e)), and a missing module keeps CPython's message;
# micro-monty's redirect sentence after it is only in the report too
# (docs/sdk.md "Redirect messages"), so the caught text matches CPython.
import json
import math
import os

for mod, name in ((math, 'sqr'), (os, 'pathx'), (json, 'dumsp'), (json, 'zzzzzz')):
    try:
        getattr(mod, name)
    except AttributeError as e:
        print(type(e).__name__, e)

try:
    import numpy
except ModuleNotFoundError as e:
    print(type(e).__name__, e)
try:
    from requests import get
except ModuleNotFoundError as e:
    print(type(e).__name__, e)
try:
    import yaml.loader
except ModuleNotFoundError as e:
    print(type(e).__name__, e)

print(callable(help))
json.lods('[]')
