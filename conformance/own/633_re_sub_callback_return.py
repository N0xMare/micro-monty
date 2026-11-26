import re

for text, pattern, empty in (('aba', 'a', ''), (b'aba', b'a', b'')):
    assert re.sub(pattern, lambda m: None, text) == text[1:2]
    assert re.sub(pattern, lambda m: empty, text) == text[1:2]
    for falsy in (False, 0, []):
        try:
            re.sub(pattern, lambda m: falsy, text)
        except TypeError as exc:
            print(exc)
        else:
            raise AssertionError(falsy)
