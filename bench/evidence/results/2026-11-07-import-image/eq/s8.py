import json
#---
import re
#---
p = re.compile(r"(?P<k>\w+)=(?P<v>\d+)")
m = p.match("ab=12")
#---
import collections
P = collections.namedtuple("P", "a b")
o = collections.OrderedDict(x=P(1, 2))
#---
m.group("k"), o, json.dumps({"a": list(P(3, 4))})
