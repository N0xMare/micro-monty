fs = []
for i in range(3):
    fs.append(lambda: i)
print([f() for f in fs])

fs = [lambda i=i: i for i in range(3)]
print([f() for f in fs])

def make():
    out = []
    for j in range(3):
        def inner():
            return j * 10
        out.append(inner)
    j = 100
    return out

print([f() for f in make()])

def counter():
    n = 0
    def inc():
        nonlocal n
        n += 1
        return n
    return inc

c = counter()
c(); c()
print(c())
d = counter()
print(d(), c())
