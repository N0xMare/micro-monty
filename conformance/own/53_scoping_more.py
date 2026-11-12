def outer():
    x = 1
    def mid():
        def inner():
            return x
        return inner
    return mid()()
print(outer())

def f():
    def g():
        nonlocal y
        y = 2
    y = 1
    g()
    return y
print(f())

def comp_scope():
    a = 10
    return [[a + i + j for j in range(2)] for i in range(2)]
print(comp_scope())

def comp_cell():
    x = 5
    fs = [lambda: x + i for i in range(3)]
    x = 100
    return [h() for h in fs]
print(comp_cell())

def class_like():
    funcs = {}
    def register(name):
        def deco(fn):
            funcs[name] = fn
            return fn
        return deco
    @register("a")
    def fa():
        return "A"
    @register("b")
    def fb():
        return "B"
    return {k: v() for k, v in funcs.items()}
print(class_like())

g_count = 0
def gfun():
    global g_count
    for _ in range(3):
        g_count += 1
    del g_count
gfun()
try:
    g_count
except NameError as e:
    print(e)

def param_shadow(len):
    return len
print(param_shadow(5))

def walrus_scope():
    [z := i for i in range(3)]
    return z
print(walrus_scope())

def loop_var_after():
    for i in range(3):
        pass
    return i
print(loop_var_after())

def empty_loop():
    for i in []:
        pass
    try:
        return i
    except UnboundLocalError as e:
        return str(e)
print(empty_loop())

def except_var():
    e = "before"
    try:
        raise ValueError
    except ValueError as e:
        pass
    try:
        return e
    except UnboundLocalError as err:
        return "unbound: " + str(err)
print(except_var())

x = 0
def reads_then_global():
    global x
    x = x + 1
    return x
print(reads_then_global(), reads_then_global())

def default_mutation(a, d={}):
    d[a] = len(d)
    return d
default_mutation("p")
print(default_mutation("q"))

def kwdefault(*, k=[]):
    k.append(1)
    return k
kwdefault()
print(kwdefault())
print((lambda: (lambda: (lambda: "deep")())())())
counter = [0]
def bump():
    counter[0] += 1
bump(); bump()
print(counter)
