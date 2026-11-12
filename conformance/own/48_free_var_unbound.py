def outer():
    def show():
        return later
    try:
        show()
    except NameError as e:
        print(e)
    later = 1
    print(show())
outer()
