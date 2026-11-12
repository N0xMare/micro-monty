def trace(f):
    def wrapper(*args, **kwargs):
        print("call", f.__name__, args)
        return f(*args, **kwargs)
    return wrapper

def times(n):
    def deco(f):
        def w(x):
            for _ in range(n):
                x = f(x)
            return x
        return w
    return deco

@trace
@times(3)
def double(x):
    return x * 2

print(double(1))
print(double.__name__, double.__qualname__)

def outer():
    def inner():
        pass
    return inner

print(outer().__qualname__)
print(repr(len), type(len), type(double))
