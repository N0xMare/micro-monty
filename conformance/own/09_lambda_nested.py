compose = lambda f, g: lambda x: f(g(x))
inc = lambda x: x + 1
dbl = lambda x: x * 2
print(compose(inc, dbl)(5), compose(dbl, inc)(5))

def adder(a):
    def b(bv):
        def c(cv):
            return a + bv + cv
        return c
    return b

print(adder(1)(2)(3))
print(sorted([3, 1, 2]), list(map(lambda v: v, [])) if False else "skip")
k = (lambda: (yy := 5) + yy)()
print(k)
