# User exception classes: hierarchy, args, __str__, custom __init__, chaining.
class AppError(Exception):
    pass


class DbError(AppError):
    def __init__(self, table, code=0):
        super().__init__(f'db failure on {table}')
        self.table = table
        self.code = code


class Timeout(DbError):
    def __str__(self):
        return f'timeout[{self.table}] after {self.code}s'


class Quiet(AppError):
    def __init__(self):
        pass


for exc in [AppError('plain'), DbError('users', 3), Timeout('jobs', 30), Quiet(), AppError(1, 2)]:
    try:
        raise exc
    except DbError as e:
        print('DbError', type(e).__name__, e, e.args, e.table)
    except AppError as e:
        print('AppError', type(e).__name__, str(e), repr(e), e.args)

print(issubclass(Timeout, Exception), Timeout.__mro__[1:4])
print(isinstance(Timeout('t'), (KeyError, AppError)))

try:
    try:
        {}['k']
    except KeyError as e:
        raise DbError('cache') from e
except AppError as e:
    print(repr(e.__cause__), e.__suppress_context__)

try:
    raise Timeout
except Timeout as e:
    print('instantiated without args?', type(e).__name__)
except TypeError as e:
    print('TypeError', e)


class WithRepr(ValueError):
    def __repr__(self):
        return 'WithRepr!'


print(repr(WithRepr('x')), str(WithRepr('x')), [WithRepr()])


class MyKey(KeyError):
    pass


try:
    raise MyKey('missing')
except LookupError as e:
    print(str(e), repr(e))


def matcher(e):
    try:
        raise e
    except (DbError, ZeroDivisionError) as x:
        return 'first'
    except Exception:
        return 'second'


print(matcher(Timeout('a')), matcher(ZeroDivisionError()), matcher(AppError()))


class Stop(StopIteration):
    pass


print(Stop(5).value, StopIteration(1, 2).value)
e = DbError('t')
e.extra = 'attr'
print(e.extra, sorted(vars(e)))


class Ctx(Exception):
    def __init__(self, *args, **kwargs):
        super().__init__(*args)
        self.kw = kwargs


print(Ctx(1, x=2).kw, Ctx(1, x=2).args)
try:
    raise ValueError('a')
except ValueError:
    try:
        raise Timeout('inner', 5)
    except AppError as inner:
        print(repr(inner.__context__))


def boom():
    raise Timeout('deep', 9)


class Service:
    def run(self):
        try:
            boom()
        except DbError as e:
            raise AppError('service failed') from e


Service().run()
