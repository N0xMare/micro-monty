# `OSError(errno, strerror, ...)` gives the subclass that stands for the
# errno, as CPython's `OSError.__new__` does, for the subclasses
# micro-monty has; a subclass called itself keeps its class.
for no in [0, 1, 2, 5, 13, 17, 20, 21, 22, 28, 110]:
    e = OSError(no, 'x')
    print(no, type(e).__name__, repr(e), str(e), e.errno, e.strerror, e.args)

for args in [(2,), (2, 'x', 'f'), (2, 'x', 'f', None, 'g'), (2, 'x', 'f', 1, 'g', 6),
             ('2', 'x'), (2.0, 'x'), (True, 'x'), (False, 'x'), (), ('a',), (2 ** 70, 'x')]:
    e = OSError(*args)
    print(len(args), type(e).__name__, str(e), e.errno, e.strerror, e.filename, e.filename2)


class Sub(OSError):
    pass


class WithInit(OSError):
    def __init__(self, *a):
        pass


print(type(Sub(2, 'x')).__name__, type(WithInit(2, 'x')).__name__)
print(type(IOError(2, 'x')).__name__, type(EnvironmentError(13, 'x')).__name__)
print(type(FileNotFoundError(13, 'x')).__name__, type(PermissionError(2, 'x')).__name__)

try:
    raise OSError(2, 'No such file or directory', 'file.txt')
except FileNotFoundError as e:
    print('caught', e, isinstance(e, OSError))

try:
    raise OSError(13, 'Permission denied')
except (FileNotFoundError, TimeoutError):
    print('wrong handler')
except PermissionError as e:
    print('caught', e)

try:
    raise OSError(5, 'Input/output error')
except (FileNotFoundError, PermissionError, FileExistsError, NotADirectoryError,
        IsADirectoryError, TimeoutError):
    print('wrong handler')
except OSError as e:
    print('caught', type(e).__name__, e)
