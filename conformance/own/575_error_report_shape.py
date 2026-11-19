# The uncaught exception's report, as a host shows it to a model: CPython
# 3.14's shape (header, frames, `Type: message` last), both chaining forms,
# `add_note` notes after the message, and recursion collapsed only where
# CPython collapses it (identical consecutive lines). docs/user-guide.md,
# "The error report". Every case is also checked parked, snapshotted and
# restored by the durability gate, which compares the reports.
import sys
import traceback


def show(f):
    try:
        f()
    except BaseException as e:
        traceback.print_exception(e, file=sys.stdout)


def deep(n):
    return deep(n + 1)


def ping(n):
    return pong(n + 1)


def pong(n):
    return pang(n + 1)


def pang(n):
    return ping(n + 1)


def chain(n):
    if n == 0:
        raise ValueError('bottom')
    return chain(n - 1) if n % 2 else chain2(n - 1)


def chain2(n):
    return chain(n)


sys.setrecursionlimit(80)
show(lambda: deep(0))
show(lambda: ping(0))
show(lambda: chain(12))


class AppError(Exception):
    pass


def load():
    try:
        {}['key']
    except KeyError as e:
        raise AppError('cannot load') from e


def run():
    try:
        load()
    except AppError:
        1 / 0


def main():
    try:
        run()
    except ZeroDivisionError as e:
        e.add_note('while running main')
        e.add_note('a note\nof two lines')
        raise


main()
