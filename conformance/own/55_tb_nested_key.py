def a():
    b()
def b():
    c = {"k": 1}
    return c["missing"] + 1
a()
