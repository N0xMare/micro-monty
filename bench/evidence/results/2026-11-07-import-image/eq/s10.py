import json
j0 = json
try:
    import nosuch
except ImportError as e:
    err = type(e).__name__
#---
import json as j1, re, collections
from json import dumps
j0 is j1, dumps is json.dumps, err, json.__name__, sorted(k for k in vars(re) if not k.startswith("_"))[:4]
#---
del json, re, collections
import collections, re, json
j0 is json, collections.OrderedDict.__module__, re.compile("a").pattern
