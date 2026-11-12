# A class's special methods are found by name, whatever str object names
# them: names built at run time (not interned), near misses that are not
# special methods, and builtin bases that define one before a later base.
eq = '__' + 'eq' + '__'
ln = ''.join(['__', 'len', '__'])

C = type('C', (), {eq: lambda s, o: True, ln: lambda s: 3, '__eq_': 1, '_eq__': 2, '__lenx__': 4})
c = C()
print(c == 1, len(c), C.__eq_, C._eq__, C.__lenx__)

class D:
    pass

d = D()
setattr(D, '__' + 'add' + '__', lambda s, o: ('add', o))
setattr(D, '__' + 'radd' + '__', lambda s, o: ('radd', o))
setattr(D, ''.join(['__', 'iter', '__']), lambda s: iter('ab'))
print(d + 1, 1 + d, list(d))
setattr(D, '__ad' + 'd_', None)
print(d + 2)
delattr(D, '__' + 'add' + '__')
try:
    d + 3
except TypeError as e:
    print(e)
D.__dict__  # the class dict is unchanged by the lookups
print(sorted(k for k in D.__dict__ if k.startswith('__a') or k.startswith('__r')))


class Mixin:
    def __len__(self):
        return 99

    def __contains__(self, x):
        return x == 'mixin'


class M(dict, Mixin):
    pass


m = M(a=1, b=2)
print(len(m), 'a' in m, 'mixin' in m)


class L(Mixin, list):
    pass


print(len(L([1, 2])), 'mixin' in L([1]))
for name in ('__len__', '__contains__'):
    setattr(Mixin, name, lambda s, *a: 7)
print(len(m), len(L()), 'x' in L())
