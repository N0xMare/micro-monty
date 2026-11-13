# namedtuple's name checks in CPython's order: ValueError for invalid
# identifiers, keywords, underscores and repeats, too many defaults a
# TypeError, then the SyntaxError its compile of `__new__` raises for
# fields NFKC makes equal (with or without rename). A field whose NFKC
# micro-monty does not carry, such as a combining mark, is kept as
# written; a field named like a tuple method is the field;
# typing.NamedTuple builds the same.
from collections import namedtuple
import typing

CASES = [
    ("my-rec", ["a"]), ("R", ["a", "1b"]), ("R", ["a", ""]), ("class", ["a"]),
    ("R", ["a", "None"]), ("R", ["a", "_b"]), ("R", ["a", "b", "a"]),
    ("_R", ["a", "match", "été"]), ("R", ["_a", "def"]), ("R", ["a", "a", "1x"]),
    ("R", ["ﬁ", "fi"]), ("R", ["a", "ﬁ", "b", "fi"]), ("R", ["x²"]), ("R", ["a·b"]),
    ("R", ["count", "index"]), ("ℛ", ["a"]), ("R", ["ﬃ", "ｍ", "m", "ffi"]),
    ("R", ["ｉｆ"]), ("R", "ﬁ fi"), ("T", ["café", "a\u0301"]), ("R", ["a", "ℌ"]),
]
for name, fields in CASES:
    for rename in (False, True):
        try:
            r = namedtuple(name, fields, rename=rename)(*range(len(fields)))
            print(repr(r), [getattr(r, f) for f in r._fields], r.count if 'count' in r._fields else '')
        except ValueError as e:
            print('ValueError', e)
        except SyntaxError as e:
            print('SyntaxError', e, e.filename, e.lineno, e.offset, e.end_lineno, e.end_offset)
for fields in (["ﬁ", "fi"], ["a", "b"]):
    try:
        print(namedtuple("R", fields, defaults=[1, 2, 3])._fields)
    except TypeError as e:
        print('TypeError', e)
for fields in (["ﬁ", "fi"], ["a", "ｂ", "b"], ["ｘ"]):
    try:
        print(typing.NamedTuple("T", [(f, int) for f in fields])._fields)
    except SyntaxError as e:
        print('SyntaxError', e, e.offset, e.end_offset)
