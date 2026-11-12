# A metaclass that defines `__prepare__`: CPython calls it for the class
# namespace; micro-monty refuses the class with a clear NotImplementedError
# instead of ignoring the hook (docs/spec.md §4). Both print the same lines.
calls = []


class Ordered(type):
    @classmethod
    def __prepare__(mcls, name, bases, **kwargs):
        calls.append(name)
        return {}


try:
    class A(metaclass=Ordered):
        x = 1
    outcome = calls == ['A'] and A.x == 1
except NotImplementedError as e:
    outcome = str(e).startswith('micro-monty does not support __prepare__')
print('prepare handled:', outcome)


# A metaclass without `__prepare__` is unaffected, and so is a subclass of a
# class whose metaclass has none.
class Plain(type):
    pass


class B(metaclass=Plain):
    y = 2


class C(B):
    pass


print(type(C).__name__, C.y)


# Inherited `__prepare__` counts too.
class Sub(Ordered):
    pass


try:
    class D(metaclass=Sub):
        pass
    outcome = calls == ['A', 'D'] or calls == ['D']
except NotImplementedError as e:
    outcome = 'does not support __prepare__' in str(e)
print('inherited prepare handled:', outcome)
