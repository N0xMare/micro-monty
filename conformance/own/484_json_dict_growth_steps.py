# `json.loads` grows a big object's table a slice per step (docs/spec.md §5,
# `Vm::dict_grow_slice`): a step ends before its entries could fill the
# table, and the next steps build the bigger table while nothing writes to
# the object; an object whose child spans a step grows before the child
# closes into it. Results, key order and duplicate keys must match CPython.
import json

d = {str(i): [i, i % 7] for i in range(100_000)}
d2 = json.loads(json.dumps(d))
print(len(d2), d2 == d, list(d2) == list(d), list(d2)[-2:], d2['99999'])

# Duplicate keys: the last value wins, at the first key's place.
text = '{' + ', '.join('"k%d": %d' % (i % 70_000, i) for i in range(90_000)) + '}'
d3 = json.loads(text)
print(len(d3), list(d3)[:2], d3['k0'], d3['k19999'], d3['k20000'], list(d3)[-1])

# A big object inside a list, with a sibling after it.
v = json.loads(json.dumps([{'n%d' % i: {'v': i} for i in range(80_000)}, 'tail']))
print(len(v[0]), v[1], v[0]['n79999'], list(v[0])[40_000])
