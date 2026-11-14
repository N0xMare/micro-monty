# The correctness review's AC1 reproducer: 180 MB passed as a call's fourth
# argument and never bound to a name is free once the statement has ended.
# python3.14 prints "ok 3".
import gc
def load():
    return [bytearray(60_000_000) for _ in range(3)]
def summarize(tag, a, b, data):
    return len(data)
def main():
    n = summarize('t', 0, 0, load())      # the data is never bound to a name
    gc.collect()
    try:
        x = bytearray(60_000_000); y = bytearray(60_000_000)
        print('ok', n)
    except MemoryError:
        print('MemoryError: the 180 MB passed to summarize() is still held')
main()
