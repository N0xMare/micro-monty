# __import__(name, globals=None, locals=None, fromlist=(), level=0) through
# the import statement's machinery: bundled modules (a body runs at its first
# import, as a call), dotted names giving the top package unless fromlist is
# given, fromlist submodules of packages, and CPython's errors.


def show(label, f):
    try:
        r = f()
        r = getattr(r, '__name__', r)
    except BaseException as e:
        r = f'{type(e).__name__}: {e}'
    print(label, '->', r)


# The forms model code writes: an expression import of a bundled module.
print(__import__('json').loads('[1, 2]'), __import__("re").sub(r'\d', '#', 'a1b22'))
data = __import__('struct').pack('<HI', 1, 2)
print(len(data), __import__('base64').b64encode(data))
print(__import__('math').sqrt(16.0), __import__('sys').platform)
if (asyncio := __import__('asyncio')):
    async def main():
        await asyncio.sleep(0)
        return 'ran'
    print(asyncio.run(main()))
counter = __import__('collections').Counter('abcab')
print(counter.most_common(1))
dt = __import__('datetime')
print(dt.date(2026, 3, 15).isoformat(), dt is __import__('datetime'))

show('json', lambda: __import__('json'))
show('textwrap first import', lambda: __import__('textwrap').dedent('  x'))
show('collections.abc', lambda: __import__('collections.abc'))
show('collections.abc same', lambda: __import__('collections.abc') is __import__('collections'))
show('collections.abc fl', lambda: __import__('collections.abc', fromlist=['Mapping']))
show('collections fl abc', lambda: __import__('collections', fromlist=['abc']).abc)
show('json.decoder fl', lambda: __import__('json.decoder', fromlist=['JSONDecoder']).JSONDecoder)
show('urllib.parse', lambda: __import__('urllib.parse'))
show('urllib.parse fl', lambda: __import__('urllib.parse', fromlist=['quote']).quote('a b'))
show('urllib fl parse', lambda: __import__('urllib', fromlist=['parse']).parse.unquote('a%20b'))
show('fl missing', lambda: __import__('collections', fromlist=['nope']))
show('math fl missing', lambda: __import__('math', fromlist=['nope']))
show('fl star', lambda: __import__('json', fromlist=['*']))
show('fl int', lambda: __import__('json', fromlist=[1]))
show('fl int nonpkg', lambda: __import__('math', fromlist=[1]))
show('fl tuple', lambda: __import__('collections', fromlist=('abc',)).abc.Sequence)
show('fl empty', lambda: __import__('collections.abc', fromlist=[]))
show('fl None', lambda: __import__('json', fromlist=None))
show('positional', lambda: __import__('json', None, None, ['loads'], 0))
show('globals junk', lambda: __import__('json', 5, 6))
show('name kw', lambda: __import__(name='json'))
show('str sub', lambda: __import__(type('S', (str,), {})('json')))
show('nope', lambda: __import__('nope'))
show('json.nope', lambda: __import__('json.nope'))
show('math.nope', lambda: __import__('math.nope'))
show('trailing dot', lambda: __import__('json.'))
show('empty', lambda: __import__(''))
show('dot', lambda: __import__('.json'))
show('int', lambda: __import__(1))
show('none', lambda: __import__(None))
show('noargs', lambda: __import__())
show('level -1', lambda: __import__('json', level=-1))
show('level 1', lambda: __import__('json', level=1))
show('level 1 g', lambda: __import__('json', globals(), level=1))
show('level str', lambda: __import__('json', level='x'))
show('too many', lambda: __import__('json', None, None, [], 0, 1))
show('bad kw', lambda: __import__('json', foo=1))
show('repr', lambda: repr(__import__))
show('callable', lambda: (__import__.__name__, callable(__import__)))

try:
    __import__('nope.sub')
except ModuleNotFoundError as e:
    print('name', e.name)
