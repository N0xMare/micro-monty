# Comparisons the native walk hands to the prelude twins: builtin-subclass
# operands whose items need a user `__eq__`, dict views against sets and
# against each other, and long runs of identical items.


class E:
    def __eq__(self, other):
        return True

    __hash__ = object.__hash__


class N:
    def __eq__(self, other):
        return False

    __hash__ = object.__hash__


class H:
    # Equal to everything, with one hash: found in sets and dicts.
    def __eq__(self, other):
        return True

    def __hash__(self):
        return 7


class L(list):
    pass


class T(tuple):
    pass


class D(dict):
    pass


# Subclasses of list, tuple and dict compare as their values.
print(L([E()]) == [E()], [E()] == L([E()]), L([E()]) == L([E()]))
print(L([E()]) != [E()], L([E()]) != L([E()]), L([N()]) != L([N()]))
print(T((E(),)) == (E(),), T((E(), 1)) == T((E(), 2)), T((N(),)) != (N(),))
print(D(a=E()) == {'a': E()}, {'a': E()} == D(a=E()), D(a=E()) != D(a=E()))
print(D(a=N()) == {'a': N()}, D(a=E()) == {'b': E()})
print([L([E()])] == [L([E()])], [D(a=E())] == [{'a': E()}])
print([L([E()])] < [L([E()])], [L([E()])] <= [L([E()])])
print([L([E(), 1])] < [L([E(), 2])], [T((E(), 3))] > [T((E(), 2))])
print(L([E()]) < L([E()]), L([E()]) <= L([E()]), L([E(), 1]) < [E(), 2])
print(T((E(), 1)) >= (E(), 1), T((E(), 1)) > T((E(),)))
try:
    L([E(), 1]) < L([E(), 'x'])
except TypeError as e:
    print('TypeError', e)

# Dict views against sets, and keys views against items views.
print({H(): 1}.keys() == {H()}, {H()} == {H(): 1}.keys())
print({(1, H())} == {1: H()}.items(), {1: H()}.items() == {(1, H())})
print({(1, 2): 0}.keys() == {1: 2}.items(), {1: 2}.items() == {(1, 2): 0}.keys())
print({}.keys() == {}.items(), {}.items() == {}.keys())
print({1: 2}.keys() == {1: 2}.items(), {(1, 2): 0}.keys() != {1: 2}.items())
print({(1, H()): 0}.keys() == {1: 2}.items(), {(1, 2): 0}.keys() == {1: H()}.items())
print({H(): 1}.keys() != {H()}, {N(): 1}.keys() == {N()})
print({(1, 2): 0}.keys() <= {1: 2}.items(), {(1, 2): 0}.keys() < {1: 2}.items())
print({1: 2}.items() >= {(1, 2): 0}.keys(), {}.keys() < {1: 2}.items())

# Long runs of identical items still compare right.
a = [0] * 100000
b = a[:]
b[-1] = 1
print(a == b, a < b, a[:-1] == b[:-1], [a] == [a[:]], (a, 1) < (b, 0))
c = [H()] + a
d = [H()] + b
print(c == d, c[:-1] == d[:-1], c < d)
