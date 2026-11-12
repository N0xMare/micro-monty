x = "global"

def outer():
    x = "outer"
    def inner():
        nonlocal x
        x = "inner"
        def deepest():
            global x
            x = "set by deepest"
        deepest()
    inner()
    return x

print(outer(), x)

def a():
    v = 1
    def b():
        def c():
            nonlocal v
            v += 10
            return v
        return c()
    return b(), v

print(a())

def shadow():
    len = lambda s: 42
    return len("abc")

print(shadow(), len("abc"))

total = 0
def add(n):
    global total
    total += n
for k in range(5):
    add(k)
print(total)
