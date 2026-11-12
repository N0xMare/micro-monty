def cleanup():
    raise OSError("cleanup failed")

def work():
    try:
        [1, 2, 3][10]
    except IndexError:
        cleanup()

work()
