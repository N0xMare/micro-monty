def inner():
    return 1 / 0

def outer():
    try:
        inner()
    except ZeroDivisionError:
        print("logging and re-raising")
        raise

outer()
