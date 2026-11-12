# generators: a three-stage generator pipeline, `yield from`, and generator
# expressions consumed by sum, any and a for loop.
def numbers(n):
    i = 0
    while i < n:
        yield i
        i += 1


def evens(source):
    for x in source:
        if x % 2 == 0:
            yield x


def scaled(source, k):
    for x in source:
        yield x * k


def chunks(n, size):
    for start in range(0, n, size):
        yield from range(start, min(start + size, n))


pipeline = 0
for x in scaled(evens(numbers(300_000)), 3):
    pipeline += x

squares = sum(x * x % 1000 for x in range(300_000))
chunked = sum(x % 7 for x in chunks(100_000, 64))
found = 0
for limit in range(100):
    if any(x > limit * 100 for x in range(10_000)):
        found += 1
print(pipeline, squares, chunked, found)
