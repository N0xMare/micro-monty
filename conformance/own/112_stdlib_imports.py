# Bundled Python modules: lazy import, one module object per session,
# from-imports, and lazy (generator-like) iterators.
import collections
import collections as c2
import itertools
from functools import reduce, partial
from itertools import islice, count

print(collections is c2, collections.__name__, itertools.__name__)
import heapq, bisect
print(heapq.nsmallest(2, [5, 1, 4]), bisect.bisect([1, 2, 4], 3))
print(reduce(lambda a, b: a * b, range(1, 6)), partial(int, base=2)('101'))
print(list(islice(count(10, 5), 4)))

evens = (n for n in count() if n % 2 == 0)
print(list(islice(evens, 3)), next(evens))
gen = itertools.chain('ab', [1, 2])
print(next(gen), list(gen))
print(list(itertools.accumulate([1, 2, 3, 4])), list(itertools.pairwise('abc')))
print([(k, list(g)) for k, g in itertools.groupby('aabccc')])
print(list(itertools.product('ab', repeat=2)), list(itertools.combinations(range(4), 2))[:3])
print(list(itertools.permutations('abc', 2))[:4], list(itertools.zip_longest('ab', 'xyz', fillvalue='-')))
print(list(itertools.batched(range(7), 3)), list(itertools.starmap(pow, [(2, 3), (3, 2)])))
a, b = itertools.tee(iter([1, 2, 3]))
print(list(a), list(b), list(itertools.compress('abcd', [1, 0, 1, 0])))
print(list(itertools.takewhile(lambda x: x < 3, [1, 2, 5, 1])), list(itertools.dropwhile(lambda x: x < 3, [1, 2, 5, 1])))
print(list(itertools.filterfalse(None, [0, 1, '', 'a'])), list(itertools.repeat('x', 3)))

try:
    import collections.nothing
except ModuleNotFoundError as e:
    print('ModuleNotFoundError', e)
try:
    from itertools import nothing
except ImportError as e:
    print('ImportError', str(e).split(' (')[0])
