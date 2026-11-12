try:
    r = 10.0 ** 400
    print('10.0 ** 400', '=>', repr(r))
except Exception as e:
    print('10.0 ** 400', '->', type(e).__name__ + ':', e)
try:
    r = (-10.0) ** 401
    print('(-10.0) ** 401', '=>', repr(r))
except Exception as e:
    print('(-10.0) ** 401', '->', type(e).__name__ + ':', e)
try:
    r = 2.0 ** 10000
    print('2.0 ** 10000', '=>', repr(r))
except Exception as e:
    print('2.0 ** 10000', '->', type(e).__name__ + ':', e)
try:
    r = f"{1:10xyz}"
    print('f"{1:10xyz}"', '=>', repr(r))
except Exception as e:
    print('f"{1:10xyz}"', '->', type(e).__name__ + ':', e)
try:
    r = f"{'s':=5}"
    print('f"{\'s\':=5}"', '=>', repr(r))
except Exception as e:
    print('f"{\'s\':=5}"', '->', type(e).__name__ + ':', e)
try:
    r = f"{1:,x}"
    print('f"{1:,x}"', '=>', repr(r))
except Exception as e:
    print('f"{1:,x}"', '->', type(e).__name__ + ':', e)
try:
    r = f"{'abc':abc}"
    print('f"{\'abc\':abc}"', '=>', repr(r))
except Exception as e:
    print('f"{\'abc\':abc}"', '->', type(e).__name__ + ':', e)
try:
    r = f"{1.5:d}"
    print('f"{1.5:d}"', '=>', repr(r))
except Exception as e:
    print('f"{1.5:d}"', '->', type(e).__name__ + ':', e)
try:
    r = f"{'s':d}"
    print('f"{\'s\':d}"', '=>', repr(r))
except Exception as e:
    print('f"{\'s\':d}"', '->', type(e).__name__ + ':', e)
try:
    r = f"{1:s}"
    print('f"{1:s}"', '=>', repr(r))
except Exception as e:
    print('f"{1:s}"', '->', type(e).__name__ + ':', e)
try:
    r = format(1, "_,")
    print('format(1, "_,")', '=>', repr(r))
except Exception as e:
    print('format(1, "_,")', '->', type(e).__name__ + ':', e)
try:
    r = sorted()
    print('sorted()', '=>', repr(r))
except Exception as e:
    print('sorted()', '->', type(e).__name__ + ':', e)
try:
    r = sorted(1,2)
    print('sorted(1,2)', '=>', repr(r))
except Exception as e:
    print('sorted(1,2)', '->', type(e).__name__ + ':', e)
try:
    r = [].pop("a")
    print('[].pop("a")', '=>', repr(r))
except Exception as e:
    print('[].pop("a")', '->', type(e).__name__ + ':', e)
try:
    r = enumerate()
    print('enumerate()', '=>', repr(r))
except Exception as e:
    print('enumerate()', '->', type(e).__name__ + ':', e)
try:
    r = zip(foo=1)
    print('zip(foo=1)', '=>', repr(r))
except Exception as e:
    print('zip(foo=1)', '->', type(e).__name__ + ':', e)
try:
    r = min(1, foo=2)
    print('min(1, foo=2)', '=>', repr(r))
except Exception as e:
    print('min(1, foo=2)', '->', type(e).__name__ + ':', e)
try:
    r = sum([], foo=1)
    print('sum([], foo=1)', '=>', repr(r))
except Exception as e:
    print('sum([], foo=1)', '->', type(e).__name__ + ':', e)
try:
    r = enumerate([], foo=1)
    print('enumerate([], foo=1)', '=>', repr(r))
except Exception as e:
    print('enumerate([], foo=1)', '->', type(e).__name__ + ':', e)
try:
    r = int("1", foo=2)
    print('int("1", foo=2)', '=>', repr(r))
except Exception as e:
    print('int("1", foo=2)', '->', type(e).__name__ + ':', e)
try:
    r = float(x=1)
    print('float(x=1)', '=>', repr(r))
except Exception as e:
    print('float(x=1)', '->', type(e).__name__ + ':', e)
try:
    r = str.join("", 5)
    print('str.join("", 5)', '=>', repr(r))
except Exception as e:
    print('str.join("", 5)', '->', type(e).__name__ + ':', e)
try:
    r = "".join([1])
    print('"".join([1])', '=>', repr(r))
except Exception as e:
    print('"".join([1])', '->', type(e).__name__ + ':', e)
try:
    r = print(**{"a": 1})
    print('print(**{"a": 1})', '=>', repr(r))
except Exception as e:
    print('print(**{"a": 1})', '->', type(e).__name__ + ':', e)
try:
    r = next(iter([]), 1, 2)
    print('next(iter([]), 1, 2)', '=>', repr(r))
except Exception as e:
    print('next(iter([]), 1, 2)', '->', type(e).__name__ + ':', e)
try:
    r = dict(1)
    print('dict(1)', '=>', repr(r))
except Exception as e:
    print('dict(1)', '->', type(e).__name__ + ':', e)
try:
    r = list(1)
    print('list(1)', '=>', repr(r))
except Exception as e:
    print('list(1)', '->', type(e).__name__ + ':', e)
try:
    r = tuple(1)
    print('tuple(1)', '=>', repr(r))
except Exception as e:
    print('tuple(1)', '->', type(e).__name__ + ':', e)
try:
    r = iter()
    print('iter()', '=>', repr(r))
except Exception as e:
    print('iter()', '->', type(e).__name__ + ':', e)
try:
    r = isinstance(1)
    print('isinstance(1)', '=>', repr(r))
except Exception as e:
    print('isinstance(1)', '->', type(e).__name__ + ':', e)
try:
    r = isinstance(1, 1)
    print('isinstance(1, 1)', '=>', repr(r))
except Exception as e:
    print('isinstance(1, 1)', '->', type(e).__name__ + ':', e)
try:
    r = repr()
    print('repr()', '=>', repr(r))
except Exception as e:
    print('repr()', '->', type(e).__name__ + ':', e)
try:
    r = hash([])
    print('hash([])', '=>', repr(r))
except Exception as e:
    print('hash([])', '->', type(e).__name__ + ':', e)
try:
    r = abs("a")
    print('abs("a")', '=>', repr(r))
except Exception as e:
    print('abs("a")', '->', type(e).__name__ + ':', e)
try:
    r = divmod(1)
    print('divmod(1)', '=>', repr(r))
except Exception as e:
    print('divmod(1)', '->', type(e).__name__ + ':', e)
try:
    r = range()
    print('range()', '=>', repr(r))
except Exception as e:
    print('range()', '->', type(e).__name__ + ':', e)
try:
    r = range(1.5)
    print('range(1.5)', '=>', repr(r))
except Exception as e:
    print('range(1.5)', '->', type(e).__name__ + ':', e)
try:
    r = chr(-1)
    print('chr(-1)', '=>', repr(r))
except Exception as e:
    print('chr(-1)', '->', type(e).__name__ + ':', e)
try:
    r = ord("ab")
    print('ord("ab")', '=>', repr(r))
except Exception as e:
    print('ord("ab")', '->', type(e).__name__ + ':', e)
try:
    r = int(1.5, 10)
    print('int(1.5, 10)', '=>', repr(r))
except Exception as e:
    print('int(1.5, 10)', '->', type(e).__name__ + ':', e)
try:
    r = int("z", 99)
    print('int("z", 99)', '=>', repr(r))
except Exception as e:
    print('int("z", 99)', '->', type(e).__name__ + ':', e)
try:
    r = [1,2].index(5)
    print('[1,2].index(5)', '=>', repr(r))
except Exception as e:
    print('[1,2].index(5)', '->', type(e).__name__ + ':', e)
try:
    r = (1,2).index(5)
    print('(1,2).index(5)', '=>', repr(r))
except Exception as e:
    print('(1,2).index(5)', '->', type(e).__name__ + ':', e)
try:
    r = "abc".index("z")
    print('"abc".index("z")', '=>', repr(r))
except Exception as e:
    print('"abc".index("z")', '->', type(e).__name__ + ':', e)
try:
    r = sorted([1, "a"])
    print('sorted([1, "a"])', '=>', repr(r))
except Exception as e:
    print('sorted([1, "a"])', '->', type(e).__name__ + ':', e)
try:
    r = 1 < "a"
    print('1 < "a"', '=>', repr(r))
except Exception as e:
    print('1 < "a"', '->', type(e).__name__ + ':', e)
try:
    r = [1] < ["a"]
    print('[1] < ["a"]', '=>', repr(r))
except Exception as e:
    print('[1] < ["a"]', '->', type(e).__name__ + ':', e)
try:
    r = len(1.5)
    print('len(1.5)', '=>', repr(r))
except Exception as e:
    print('len(1.5)', '->', type(e).__name__ + ':', e)
try:
    r = -"a"
    print('-"a"', '=>', repr(r))
except Exception as e:
    print('-"a"', '->', type(e).__name__ + ':', e)
try:
    r = ~1.5
    print('~1.5', '=>', repr(r))
except Exception as e:
    print('~1.5', '->', type(e).__name__ + ':', e)
try:
    r = not_defined_x
    print('not_defined_x', '=>', repr(r))
except Exception as e:
    print('not_defined_x', '->', type(e).__name__ + ':', e)
try:
    r = {}.pop(1)
    print('{}.pop(1)', '=>', repr(r))
except Exception as e:
    print('{}.pop(1)', '->', type(e).__name__ + ':', e)
try:
    r = {}.popitem()
    print('{}.popitem()', '=>', repr(r))
except Exception as e:
    print('{}.popitem()', '->', type(e).__name__ + ':', e)
try:
    r = set().pop()
    print('set().pop()', '=>', repr(r))
except Exception as e:
    print('set().pop()', '->', type(e).__name__ + ':', e)
try:
    r = [].remove(1)
    print('[].remove(1)', '=>', repr(r))
except Exception as e:
    print('[].remove(1)', '->', type(e).__name__ + ':', e)
