# ast.dump of bare generator arguments, f-string pieces and comparisons;
# comparison errors of builtin subclasses; Task.set_name of a non-str.

import ast
print(ast.dump(ast.parse('f(x for x in y)')))
print(ast.dump(ast.parse('f((x for x in y))')))
print(ast.dump(ast.parse("f'{a=}{b!r:>5}{c = }'")))
print(ast.dump(ast.parse('global a, b\ndel a, b[0]')))
print(ast.dump(ast.parse('{k: v for k, v in x}')))
print(ast.dump(ast.parse('a < b <= c')))

import ast
for s in ("f'{a}{b}x{c!r}'", "f'a{b:{c}}d'", "f'{a}' 'x' f'{b}'", "f'{x=!r:^10}'", "f'''{a}\n{b}'''"):
    print(ast.dump(ast.parse(s)))

class L(list):
    pass
class I(int):
    pass
class S(str):
    pass
import asyncio
for f in (lambda: L() < 5, lambda: L([1]) < (1,), lambda: S('a') < 1, lambda: I(1) < 'a', lambda: -S('a'), lambda: ~L(), lambda: L() - L(), lambda: S('a') @ I(1), lambda: {1}.__class__ and frozenset() < []):
    try:
        print(f())
    except TypeError as e:
        print(e)
async def main():
    t = asyncio.create_task(asyncio.sleep(0))
    t.set_name(5)
    print(t.get_name())
    await t
asyncio.run(main())
print(ast.dump(ast.parse('a > b >= c in d not in e is f is not g')))
match (1, 2):
    case 1, 2:
        print('open sequence pattern')
match (1, 2, 3):
    case 1, *rest:
        print(rest)
