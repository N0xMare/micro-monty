# operator, abc, string and textwrap.
import operator
import string
import textwrap
from abc import ABC, abstractmethod

rows = [('b', 2), ('a', 3), ('c', 1)]
print(sorted(rows, key=operator.itemgetter(1)), operator.itemgetter(0, 1)(rows[0]))
print(operator.attrgetter('real')(3), operator.methodcaller('upper')('x'), operator.add(1, 2))
print(operator.itemgetter(1), operator.attrgetter('a', 'b.c'), operator.methodcaller('f', 1, k=2))
print(operator.mul(3, 4), operator.neg(5), operator.contains([1], 1), operator.index(7), operator.length_hint([1, 2]))
print(operator.concat([1], [2]), operator.truth(0), operator.is_(None, None), operator.pow(2, 5))


class Shape(ABC):
    @abstractmethod
    def area(self):
        ...

    @abstractmethod
    def name(self):
        ...

    def describe(self):
        return f'{self.name()}: {self.area()}'


class Square(Shape):
    def __init__(self, s):
        self.s = s

    def area(self):
        return self.s ** 2

    def name(self):
        return 'square'


class Half(Shape):
    def area(self):
        return 0


print(Square(3).describe(), isinstance(Square(1), Shape))
for cls in (Shape, Half):
    try:
        cls()
    except TypeError as e:
        print('TypeError', e)

print(string.ascii_letters, string.digits, string.punctuation, string.capwords(' hello   world '))
t = string.Template('$who likes ${what}; $$5')
print(t.substitute(who='tim', what='pie'), t.safe_substitute(who='tim'), t.get_identifiers(), t.is_valid())
try:
    string.Template('bad $').substitute()
except ValueError as e:
    print('ValueError', e)
try:
    t.substitute(who='x')
except KeyError as e:
    print('KeyError', e)

text = 'The quick brown fox jumps over the lazy dog. A well-known example of a pangram sentence.'
print(textwrap.wrap(text, 20))
print(textwrap.fill(text, 30, initial_indent='> ', subsequent_indent='  '))
print(textwrap.shorten(text, 40), textwrap.shorten('Hello  world!', width=12))
print(repr(textwrap.dedent('''
    def f():
        return 1
''')))
print(repr(textwrap.indent('a\n\nb\n', '+ ')), textwrap.wrap('supercalifragilistic', 6))
