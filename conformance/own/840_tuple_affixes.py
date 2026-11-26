# Late matches and misses park inside a comparison, and a long tuple of
# short misses parks between candidates. The first matching member wins.
n = 8_400_009
text = 'é' + 'a' * n + 'z'
yes = 'a' * n
no = 'a' * (n - 1) + 'x'
assert text.startswith((no, yes, 9), 1, -1)
assert not text.endswith((no,), 1, -1)
del text, yes, no

s = bytearray(b'Z' + b'a' * n + b'!')
no = b'a' * (n - 1) + b'x'
yes = bytearray(b'a' * n)
assert s.endswith((no, yes, 9), 1, -1)
assert not s.startswith((no,), 1, -1)
assert b'a'.startswith((b'b',) * 140_000 + (b'a', 9))

for s, p, message in (
    ('a', ('b', 9), 'tuple for startswith must only contain str, not int'),
    (b'a', (b'b', 9), "a bytes-like object is required, not 'int'"),
):
    try:
        s.startswith(p, 9)
        raise AssertionError('invalid tuple member was ignored')
    except TypeError as e:
        assert str(e) == message
