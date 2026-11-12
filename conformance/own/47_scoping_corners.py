x = 1
def f():
    return [x for _ in range(2)]
print(f())

def g():
    x = 2
    def h():
        return x
    x = 3
    return h()
print(g())

def k():
    y = 1
    def m():
        return [y + i for i in range(2)]
    y = 5
    return m()
print(k())

def default_eval():
    v = 1
    def inner(a=v):
        return a
    v = 2
    return inner()
print(default_eval())

def cell_param(p):
    def get():
        return p
    p = p * 2
    return get()
print(cell_param(4))

def globs():
    global newg
    newg = "created"
globs()
print(newg)

def loop_closure():
    out = []
    for i in range(3):
        def mk(i=i):
            return lambda: i
        out.append(mk())
    return [f() for f in out]
print(loop_closure())
print([lambda: q for q in range(2)][0]())

def nonlocal_rebind():
    a = 1
    def change():
        nonlocal a
        a = "changed"
    change()
    return a
print(nonlocal_rebind())
