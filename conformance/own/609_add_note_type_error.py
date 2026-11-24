# `add_note()` of a note that is not a str raises CPython's TypeError,
# which names `None` as `None`, and adds nothing.
for note in (1, b'x', None, 2.5, [1], ValueError('v')):
    try:
        ValueError().add_note(note)
    except TypeError as e:
        print(type(e).__name__, e)

e = ValueError('base')
e.add_note('first')
e.add_note('second')
print(e.__notes__)
try:
    e.add_note(None)
except TypeError as err:
    print(err, e.__notes__)
