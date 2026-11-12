# add_note's notes under the final traceback line, and caret anchors of a
# binary operation split across lines by a comment and by a backslash.
def g():
    x = 1 \
        + None
    return x


try:
    g()
except TypeError as e:
    print(e)


def f():
    e = ValueError('boom')
    e.add_note('first note')
    e.add_note('two\nlines')
    try:
        return (1  # a comment
                + 'a')
    except TypeError as t:
        t.add_note('while adding')
        raise e from t


f()
