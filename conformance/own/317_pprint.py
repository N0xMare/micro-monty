# pprint: pprint, pformat, pp, saferepr and PrettyPrinter with width, indent,
# depth, compact, sort_dicts and underscore_numbers (docs/spec.md §4, §12).
import pprint, collections, dataclasses, types, io, sys
data = {'name': 'agent', 'tools': ['search', 'fetch', 'parse', 'summarize', 'write', 'review'] * 2,
        'config': {'depth': 3, 'nested': {'a': [1, 2, {'b': (3, 4, 5)}], 'z': None, 'm': 1.5}},
        'text': 'lorem ipsum dolor sit amet ' * 6, 'blob': bytes(range(40)), 'empty': {}, 3: 'int key'}
pprint.pprint(data)
pprint.pprint(data, width=40, indent=2)
pprint.pprint(data, depth=2)
pprint.pprint(data, sort_dicts=False, width=60)
pprint.pp(data, width=50)
print(pprint.pformat(data, compact=True, width=50))
print(pprint.pformat(list(range(40)), compact=True))
print(pprint.pformat(list(range(40)), width=30)[:120])
print(pprint.pformat(10**20, underscore_numbers=True), pprint.pformat([10**8], underscore_numbers=True))
print(pprint.saferepr(data)[:80], pprint.isreadable(data), pprint.isrecursive(data))
r = [1, 2]; r.append(r)
print(pprint.pformat(r).replace(str(id(r)), 'ID'), pprint.isrecursive(r), pprint.isreadable(r))
d = {'k': 1}; d['self'] = d
print(pprint.saferepr(d).replace(str(id(d)), 'ID'))
od = collections.OrderedDict((str(i), i * 'x') for i in range(8))
pprint.pprint(od, width=30)
pprint.pprint(collections.defaultdict(list, {'a': [1] * 20, 'b': [2] * 20}), width=40)
pprint.pprint(collections.Counter('abracadabra' * 5), width=30)
pprint.pprint(collections.deque(range(30), maxlen=40), width=40)
pprint.pprint(collections.ChainMap({'a': 'x' * 30}, {'b': 'y' * 30}), width=40)
pprint.pprint(types.SimpleNamespace(alpha='a' * 30, beta=list(range(15))), width=40)
pprint.pprint(types.MappingProxyType({'q': 'w' * 50, 'e': 1}), width=40)
pprint.pprint({frozenset({1, 2, 3}), frozenset({4})}, width=10)
pprint.pprint(set(range(30)), width=40)
pprint.pprint(tuple(range(30)), width=40)
pprint.pprint(('single' * 20,), width=40)
pprint.pprint({'mixed': [1, 'a', 2.5, None, (1,), b'x']}, width=20)
pprint.pprint({1: 'a', 'b': 2, (1,): 3, None: 4}, width=20)
@dataclasses.dataclass
class Item:
    name: str
    tags: list
    score: float = 0.0
    hidden: int = dataclasses.field(default=0, repr=False)
pprint.pprint([Item('widget', ['a' * 10, 'b' * 10, 'c' * 10], 1.5)] * 2, width=40)
class Own(dict):
    def __repr__(self): return 'Own()'
pprint.pprint({'own': Own(a=1), 'sub': type('S', (dict,), {})(x='y' * 80)}, width=40)
buf = io.StringIO()
pprint.pprint({'to': 'stream'}, stream=buf)
print(repr(buf.getvalue()))
pp = pprint.PrettyPrinter(indent=4, width=30, depth=3)
pp.pprint({'a': {'b': {'c': {'d': 1}}}, 'l': [[[[1]]]]})
print(pp.pformat('x' * 70))
print(pprint.pformat('line one\nline two that is fairly long and wraps around here\n', width=30))
print(pprint.pformat(b'\x00\x01' * 30, width=30))
for bad in [lambda: pprint.PrettyPrinter(indent=-1), lambda: pprint.PrettyPrinter(depth=0),
            lambda: pprint.PrettyPrinter(width=0)]:
    try:
        bad()
    except ValueError as e:
        print('ValueError', e)
