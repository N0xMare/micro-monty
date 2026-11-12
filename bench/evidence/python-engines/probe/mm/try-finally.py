def f():
    try:
        return 1
    finally:
        global r
        r = "finally ran"
f()
print(r)
