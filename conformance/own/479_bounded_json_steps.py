# Bounded native steps for json (docs/spec.md §5): a long `loads` reads a
# window of input a step and keeps its open containers in the arena between
# steps; a string or number too long for a window is read a slice a step.
# A long `dumps` keeps its open containers and its text so far between
# steps, writes a long str a slice a step, has the prelude sort the keys of
# a large dict, and joins its text a slice a step. Results must equal one
# native run's, and CPython's.
import json


def sig(s):
    return len(s), s[:40], s[-40:], sum(map(ord, s[::997]))


rows = [{'id': i, 'name': 'n%d' % i, 'tags': ['a', 'b"\n', 'é'], 'x': i / 4, 'ok': i % 3 == 0, 'z': None}
        for i in range(12_000)]
text = json.dumps(rows)
print(sig(text))
print(json.loads(text) == rows)
print(sig(json.dumps(rows, indent=2, ensure_ascii=False)))
print(sig(json.dumps(rows, separators=(',', ':'), sort_keys=True)))
ints = list(range(250_000))
print(sig(json.dumps(ints)), json.loads(json.dumps(ints))[-3:])
nest = [[i, [i, {'k': [i]}]] for i in range(25_000)]
print(json.loads(json.dumps(nest)) == nest)
deep = json.loads('[' * 3_000 + '1' + ', 2' * 200_000 + ']' * 3_000)
d = deep
while isinstance(d, list) and len(d) == 1:
    d = d[0]
print(len(d), d[:3])

# Long tokens: strings and numbers over a window.
long = 'abc"\\\n\té😀' * 35_000
print(json.loads(json.dumps(long)) == long, sig(json.dumps(long)), sig(json.dumps(long, ensure_ascii=False)))
print(sig(json.loads('"' + 'x' * 400_000 + '"')))
print(json.loads('[' + ' ' * 400_000 + '1]'))
f = json.loads('1.' + '25' * 160_000)
print(f, json.loads('-0.' + '0' * 300_000 + '1e400_000'.replace('_', '')))
try:
    json.loads('1' * 300_000)
except ValueError as e:
    print(type(e).__name__, str(e)[:60])
for bad in ['"' + 'a' * 600_000, '[' + '1,' * 200_000 + ']', '[' + '1,' * 200_000 + '1 2]',
            '{"a": 1, ' * 1 + '"b' + 'é' * 300_000 + '": [}', '"' + 'é' * 300_000 + '\\x"']:
    try:
        json.loads(bad)
    except json.JSONDecodeError as e:
        print('JSONDecodeError', e.msg, e.pos, e.lineno, e.colno)

# Hooks see the whole tree, children first, in document order.
pairs = json.loads(json.dumps([{'a': i, 'b': [i]} for i in range(20_000)]),
                   object_pairs_hook=lambda p: tuple(p))
print(len(pairs), pairs[:2], pairs[-1])
print(json.loads(json.dumps(ints[:100_000]), parse_int=lambda s: -int(s))[-2:])
v, end = json.JSONDecoder().raw_decode('  [' + '1, ' * 100_000 + '2] tail', 2)
print(len(v), end)

# Long dict keys and values, sorted keys, fallbacks part way.
big = {('k%06d' % ((i * 7919) % 20_000)): i for i in range(20_000)}
print(sig(json.dumps(big, sort_keys=True)), sig(json.dumps(big, sort_keys=True, indent='\t')))
print(sig(json.dumps({'x' * 9_000 + 'é': long[:9_000], 'y': 1, long: [long]})))
print(sig(json.dumps({**{str(i): i for i in range(5_000)}, 1: 2}, sort_keys=False)))
try:
    json.dumps({**{str(i): i for i in range(5_000)}, 1: 2}, sort_keys=True)
except TypeError as e:
    print('TypeError', e)
print(sig(json.dumps(ints[:140_000] + [{1, 2}], default=sorted)))
cyc = list(range(20_000))
cyc.append(cyc)
try:
    json.dumps(cyc)
except ValueError as e:
    print('ValueError', e, e.__notes__[:1])
try:
    json.dumps(ints[:20_000] + [float('nan')], allow_nan=False)
except ValueError as e:
    print('ValueError', e, e.__notes__)
print(sig(json.dumps([10 ** 300] * 500)))
