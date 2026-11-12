# json against CPython 3.14's C encoder: subclasses of str, int, float, list,
# tuple and dict encode as the value they extend (IntEnum, StrEnum,
# OrderedDict, defaultdict, Counter), a dict subclass with its own items()
# is asked for them, errors carry CPython's __notes__ path, skipkeys with
# indent leaves an empty object as {}; JSONEncoder/JSONDecoder (cls=,
# default(), raw_decode, strict=False) and the json.encoder/json.decoder
# submodules; a complete \uXXXX escape at the end of input is unterminated.
import json
from collections import OrderedDict
import enum
class C(enum.IntEnum):
    A = 1
class S(str, enum.Enum):
    X = "x"
class F(float): pass
for v in [OrderedDict(a=1), C.A, {C.A: 2}, S.X, {S.X: 1}, F(1.5), [True], {"a": OrderedDict(b=[1])}]:
    try: print(json.dumps(v))
    except Exception as e: print(type(e).__name__, e)
class E(json.JSONEncoder):
    def default(self, o):
        if isinstance(o, set): return sorted(o)
        return super().default(o)
print(json.dumps({"s": {3,1}}, cls=E))
print(E().encode({1: 2}))
print(json.JSONDecoder().decode('{"a": 1}'))
print(json.dumps({}, skipkeys=True, indent=4), json.dumps({1: 2}, indent=4))
try: json.dumps({"a": [object()]})
except TypeError as e: print(e, getattr(e, "__notes__", None))
try:
    import json.decoder
    print(json.decoder.JSONDecodeError is json.JSONDecodeError)
except Exception as e: print("E", type(e).__name__, e)
try:
    from json.decoder import JSONDecodeError
    print(JSONDecodeError)
except Exception as e: print("E", type(e).__name__, e)
try:
    from json.encoder import JSONEncoder
    print(JSONEncoder)
except Exception as e: print("E", type(e).__name__, e)
try: json.loads('"abc')
except Exception as e: print(type(e).__name__, e)
try: json.loads('["\\u12')
except Exception as e: print(type(e).__name__, e)
import json, collections
class D(dict): pass
class X: pass
for v in [collections.OrderedDict(a=object()), D(a=[1, {2}]), {"k": X()}, [float('nan')], {1: {2: 3, (1,): 4}}]:
    for kw in [{}, {"indent": 2}, {"allow_nan": False}]:
        try: print(json.dumps(v, **kw))
        except Exception as e: print(type(e).__name__, e, e.__notes__ if hasattr(e, '__notes__') else None)
try: json.dumps({"a": X()}, default=lambda o: {"x": 1j} if not isinstance(o, complex) else 1/0)
except Exception as e: print(type(e).__name__, e, e.__notes__)
l = []; l.append(l)
try: json.dumps({"a": l})
except Exception as e: print(type(e).__name__, e, e.__notes__)
import json, enum
from collections import defaultdict, Counter, OrderedDict, UserDict
from json import JSONDecoder, JSONEncoder
d = JSONDecoder()
print(d.raw_decode('{"a": 1} trailing'), d.raw_decode('xx[1, 2]yy', 2))
for s, kw in [(' 1', {}), ('', {}), ('[1', {})]:
    try: print(d.raw_decode(s, **kw))
    except json.JSONDecodeError as e: print("E", e.msg, e.pos)
try: d.raw_decode('1', -1)
except ValueError as e: print(e)
print(json.loads('{"a": "x\ny"}', strict=False))
try: json.loads('{"a": "x\ny"}')
except json.JSONDecodeError as e: print(e)
print(JSONDecoder(strict=False).decode('"\t"'))
class D(JSONDecoder):
    def __init__(self, **kw):
        super().__init__(object_hook=self.hook, **kw)
    def hook(self, o): return {k.upper(): v for k, v in o.items()}
print(json.loads('{"a": {"b": 1}}', cls=D))
print(json.loads('[1.5, 2]', parse_float=str, parse_int=lambda x: int(x) * 10))
class P(enum.IntEnum):
    B = 2; A = 1
print(json.dumps({P.B: P.A, P.A: [P.B]}, sort_keys=True), json.dumps(P.A, indent=2))
dd = defaultdict(list); dd["x"].append(1); dd["y"]
print(json.dumps(dd), json.dumps(Counter("abca"), sort_keys=True), json.dumps({"o": OrderedDict([("z", 1), ("a", 2)])}, indent=1))
class L(list): pass
class T(tuple): pass
print(json.dumps(L([1, T((2, 3))])), json.dumps({"a": 1, (1,): 2}, skipkeys=True, indent=2), json.dumps({(1,): 2}, skipkeys=True, indent=2))
class I(dict):
    def items(self): return [("fake", 1)]
print(json.dumps(I(a=2)))
try: json.dumps(UserDict(a=1))
except TypeError as e: print(e)
class E2(JSONEncoder):
    def default(self, o):
        if isinstance(o, set): return sorted(o)
        return super().default(o)
try: E2().encode({"a": object()})
except TypeError as e: print(e, e.__notes__)
try: JSONEncoder(1)
except TypeError as e: print(e)
try: JSONEncoder(foo=1)
except TypeError as e: print(e)
try: json.dumps(1, foo=1)
except TypeError as e: print(e)
try: json.loads("1", foo=1)
except TypeError as e: print(e)
import json.decoder, json.encoder
print(json.decoder.JSONDecoder is JSONDecoder, json.encoder.JSONEncoder is JSONEncoder)
print(json.dumps([float("inf"), 1e300, -0.0]), json.dumps({1.5: 2, True: 3, None: 4}))
print(list(E2().iterencode([1]))[-1][-1])
for data in ['', '[', '[42', '["spam', '{"spam":42,', '"', r'"\u0041', r'"\ud834\udd1e', r'{"a": "\u0041',
             r'"\u12', r'"\ud834\u', r'"\ud834\udd1']:
    try: json.loads(data)
    except json.JSONDecodeError as e: print(repr(data), e.msg, e.pos)

# list/tuple subclasses, sort_keys on a dict subclass (the Python encoder),
# an empty defaultdict under indent, a circular OrderedDict.
from collections import OrderedDict, defaultdict, Counter
class L(list): pass
class T(tuple): pass
print(json.dumps(OrderedDict(b=1, a=[L([1, 2]), T(), L()]), sort_keys=True), json.dumps({'x': defaultdict(list)}, indent=2), json.dumps(Counter(), sort_keys=True))
d = OrderedDict(); d['self'] = d
try: json.dumps(d)
except ValueError as e: print(e)
