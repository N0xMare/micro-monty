def values():
    yield 'start'
    try:
        for item in current:
            print('re-entered', item)
    except ValueError as exc:
        yield str(exc)
    yield 'end'


current = values()
print(list(current))
