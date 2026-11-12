# json: dumps options, round trips, decode errors and the digit limit.
import json


def attempt(f):
    try:
        print(repr(f()))
    except Exception as e:
        print(type(e).__name__, e)


data = {
    'name': 'Zoë "quoted" \\ back',
    'nums': [0, -1, 2**70, 1.5, -0.0, 1e20, 1e-7, 3.141592653589793],
    'flags': [True, False, None],
    'nested': {'empty_list': [], 'empty_dict': {}, 'tuple': (1, (2, 3))},
    'ctrl': '\x00\x1f\x7f\b\f\n\r\t',
    'astral': '😀 snow ☃',
}
print(json.dumps(data))
print(json.dumps(data, ensure_ascii=False))
print(json.dumps(data, indent=2, sort_keys=True))
print(json.dumps(data, indent='\t', separators=(',', ': ')))
print(json.dumps(data, separators=(',', ':')))
print(json.dumps([1, [2, [3]]], indent=0), json.dumps({'a': 1}, indent=-2))
print(json.dumps({3: 'int', 1.5: 'float', True: 't', None: 'n', 'z': 's'}))
print(json.dumps({'b': 1, 'a': {'d': 2, 'c': 3}}, sort_keys=True))
print(json.dumps({(1,): 1, 'ok': 2}, skipkeys=True), json.dumps('just a string'))
print(json.dumps([float('nan'), float('inf'), -float('inf')]))
print(json.dumps({1, 2}, default=sorted), json.dumps([{'x'}], default=list, indent=1))
print(json.dumps(10**4299) == str(10**4299))

# round trips
for v in [data['nums'], data['flags'], {'k': ['v', {'w': None}]}, 'é\n"', 2**100, -0.0, []]:
    text = json.dumps(v)
    back = json.loads(text)
    print(text, back == json.loads(json.dumps(back)), repr(back))
print(json.loads('  {"a" : [1 , 2.5e3 , -0 , "x\\u00e9\\ud83d\\ude00\\n"]}  '))
print(json.loads('[NaN, Infinity, -Infinity, 1E400, 12345678901234567890123]'))
print(json.loads(b'\xef\xbb\xbf{"bom": true}'), json.loads(b'"\xc3\xa9"'))
print(json.loads('{"a": 1, "a": 2}'), json.loads('"\\/\\b\\f\\r\\t"'))

# Colliding key hashes exercise growth and wraparound in the decoder's memo.
keys = ['collision-1784', 'collision-10196', 'collision-12872', 'collision-16778',
        'collision-33947', 'collision-40636', 'collision-46508', 'collision-74146',
        'collision-79612', 'collision-79968', 'collision-89405', 'collision-107989']
rows = [{key: i} for i, key in enumerate(keys)]
rows += [{key: -i} for i, key in enumerate(keys)]
decoded = json.loads(json.dumps(rows))
assert decoded == rows
for i in range(len(keys)):
    assert next(iter(decoded[i])) is next(iter(decoded[len(keys) + i]))

# Shared children cease being active when their branch finishes; ancestors do not.
for depth in (4, 9, 33, 65):
    leaf = [7]
    tree = leaf
    for _ in range(depth):
        tree = [tree, leaf]
    expected = '[' * depth + '[7]' + ', [7]]' * depth
    assert json.dumps(tree) == expected
    assert json.dumps(tree, check_circular=False) == expected
    leaf.append(tree)
    try:
        json.dumps(tree)
    except ValueError as exc:
        assert str(exc) == 'Circular reference detected'
    else:
        assert False, 'ancestor cycle was missed'
    leaf.pop()
    assert json.dumps(tree) == expected

# encoder errors
attempt(lambda: json.dumps({1}))
attempt(lambda: json.dumps({(1, 2): 3}))
attempt(lambda: json.dumps(float('nan'), allow_nan=False))
attempt(lambda: json.dumps({'k': float('-inf')}, allow_nan=False))
attempt(lambda: json.dumps({2: 1, 'b': 2}, sort_keys=True))
attempt(lambda: json.dumps(1, separators=(1, ':')))
attempt(lambda: json.dumps(1, separators=[',']))
attempt(lambda: json.dumps([1], indent=1.5))
attempt(lambda: json.dumps(1, bogus=1))
attempt(lambda: json.dumps(10**4300))
loop = []
loop.append(loop)
attempt(lambda: json.dumps(loop))
attempt(lambda: json.dumps({'a': {'b': loop}}))
attempt(lambda: json.dumps(object(), default=lambda o: o))

# decoder errors
for text in ['', ' ', '[', '[1,]', '{"a":1,}', '{"a" 1}', '{1: 2}', '[1 2]', '"abc', '"a\\x"',
             '"\\u12"', '"\\uZZZZ"', 'nul', 'tru', '-', '01', '1.', '1e', '[1]x', '"\x01"',
             '\ufeff[]', '{"a": [1, 2, {"b": }]}', '[\n  1,\n  oops\n]', '"é" x']:
    try:
        print(repr(json.loads(text)))
    except json.JSONDecodeError as e:
        print('JSONDecodeError', e, '|', e.msg, e.pos, e.lineno, e.colno)
attempt(lambda: json.loads('1' * 4301))
attempt(lambda: json.loads(5))
attempt(lambda: json.loads('1', bogus=1))
print(isinstance(json.JSONDecodeError('m', 'doc', 0), ValueError))
