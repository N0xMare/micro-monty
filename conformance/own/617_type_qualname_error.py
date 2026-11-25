class Other:
    pass


for value in (None, 123, Other()):
    try:
        type('Bad', (), {'__qualname__': value})
    except TypeError as exc:
        print(str(exc))
