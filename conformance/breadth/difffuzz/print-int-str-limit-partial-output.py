# CPython's print writes "a " before str(10**5000) raises; mm writes nothing.
try:
    print("a", 10 ** 5000)
except ValueError:
    print("EXC")
