xs = []
try:
    while True:
        xs.append('x' * 1000000)
except MemoryError:
    n = len(xs)
    xs = None
    print('caught', n > 0)
