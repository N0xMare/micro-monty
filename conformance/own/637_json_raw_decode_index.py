import json


class Index:
    def __index__(self):
        return 1


decode = json.JSONDecoder().raw_decode
assert decode('é123', Index()) == (123, 4)
assert decode(' 123', True) == (123, 4)
for index in (None, '1', 1.0, -1, 2**63):
    try:
        decode(' 123', index)
    except (TypeError, ValueError, OverflowError) as exc:
        print(type(exc).__name__, exc)
    else:
        raise AssertionError(index)

try:
    decode(' 123', 10)
except json.JSONDecodeError as exc:
    assert exc.pos == 10
else:
    raise AssertionError('past end')
