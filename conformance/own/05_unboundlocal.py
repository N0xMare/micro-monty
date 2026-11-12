x = 10

def f():
    print(x)
    x = 5

try:
    f()
except UnboundLocalError as e:
    print(type(e).__name__, e)

def g():
    del_me = 1
    del del_me
    return del_me

try:
    g()
except UnboundLocalError as e:
    print(e)

def h():
    def inner():
        return y
    try:
        inner()
    except NameError as e:
        print(type(e).__name__, e)
    y = 3
    return inner()

print(h())
print(isinstance(UnboundLocalError("x"), NameError))
f()
