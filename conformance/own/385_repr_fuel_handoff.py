# repr of large containers under limited fuel: the native walk hands the rest
# to the prelude twin, which parks on fuel; output equals an unparked run.
import asyncio
a=[0]
for _ in range(10): a=[a,a]
s=f"{a}"; print(len(s))
print(len(repr(a)), len(str(a)), len('%s' % (a,)), len('{}'.format(a)), len(format(a, '')))
b=[list(range(300)), {i: (i,) for i in range(200)}, {1,2,3}, {}.keys()]
c=[b]; c.append(c)
print(repr(c)[-60:])
class R:
    def __repr__(self): return 'R!'
d=[[R()]*500, (1,), [[]]*3]
print(repr(d)[:40], repr(d)[-40:], len(repr(d)))
try:
    {}[tuple(range(500))]
except KeyError as e:
    print(len(str(e)))
x = {'k': [1]*1000}
match x:
    case {'k': [1, *rest]}: print(len(rest))
print(len(str([b]*20)))
a=[0]
for _ in range(10): a=[a,a]
print(len('%s' % (a,)), len('%r' % (a,)), len('%a' % (a,)))
print(len(f"{a!r}"), len(f"{a!a}"), len(ascii(a)), len(list.__repr__(a)))
try: {}[a]
except TypeError as e: print('TE', len(str(e)))
try: raise KeyError(a)
except KeyError as e: print('KE', len(str(e)), len(repr(e)))
try: raise ValueError(a, a)
except ValueError as e: print('VE', len(str(e)))
async def m():
    try: asyncio.create_task(a)
    except TypeError as e: print('CT', len(str(e)))
asyncio.run(m())
try: float(a)
except TypeError as e: print('F', str(e)[:40])
print(len(str({'k': a})), len(str((a,))), len(str({a[0][0][0][0][0][0][0][0][0][0][0]})))
print(len('{!r:>10}'.format(a)), len(format(a)))
