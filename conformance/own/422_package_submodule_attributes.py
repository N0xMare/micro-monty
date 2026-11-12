# `collections.abc` and `json.decoder`/`json.encoder` are attributes of
# their packages after a plain `import collections` / `import json`, as in
# CPython (whose packages import them); misses keep their messages.
import collections
import json


def show(label, f):
    try:
        r = f()
    except Exception as e:
        r = f'{type(e).__name__}: {e}'
    print(label, r)


show('abc module', lambda: collections.abc.__name__)
show('Mapping check', lambda: isinstance({}, collections.abc.Mapping))
show('Sequence check', lambda: isinstance('s', collections.abc.Sequence))
show('Iterable check', lambda: [isinstance(x, collections.abc.Iterable) for x in (1, [], 'a', {}.items())])
show('missing attr', lambda: collections.no_such_thing)
show('missing attr name', lambda: getattr(collections, 'Countr'))
show('hasattr', lambda: (hasattr(collections, 'abc'), hasattr(collections, 'nope')))
show('getattr default', lambda: getattr(collections, 'nope', 'default'))
show('decoder', lambda: json.decoder.JSONDecodeError is json.JSONDecodeError)
show('encoder', lambda: json.encoder.JSONEncoder is json.JSONEncoder)
show('decoder class', lambda: json.decoder.JSONDecoder().decode('[1, 2]'))

try:
    json.loads('{"a": 1,}')
except json.decoder.JSONDecodeError as e:
    print('caught', type(e).__name__, e.msg, e.pos)

from collections import abc as cabc
print('from-import', cabc.MutableSequence.__name__)
try:
    from collections import Countr
except ImportError as e:
    print('ImportError', e.name)
collections.Countr
