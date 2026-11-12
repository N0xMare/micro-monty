# copy, heapq and bisect.
import bisect
import copy
import heapq
from collections import Counter, OrderedDict, deque, namedtuple


class Node:
    def __init__(self, v, kids=()):
        self.v = v
        self.kids = list(kids)


tree = Node(1, [Node(2), Node(3)])
tree.kids.append(tree)
t2 = copy.deepcopy(tree)
print(t2.v, [k.v for k in t2.kids[:2]], t2.kids[2] is t2, t2.kids[0] is not tree.kids[0])
sh = copy.copy(tree)
print(sh is not tree, sh.kids is tree.kids)

P = namedtuple('P', 'a b')
q = copy.deepcopy(P([1], 2))
print(q, type(q).__name__, copy.copy(deque([1, 2], maxlen=3)).maxlen)
print(copy.deepcopy(Counter('abca')), copy.deepcopy(OrderedDict(a=[1])))


class Custom:
    def __init__(self):
        self.data = [1]

    def __deepcopy__(self, memo):
        return 'custom'


print(copy.deepcopy([Custom()]), copy.replace(P(1, 2), b=3))

h = []
for x in [5, 1, 8, 3, 2]:
    heapq.heappush(h, x)
print([heapq.heappop(h) for _ in range(3)], h)
data = [9, 4, 7, 1]
heapq.heapify(data)
print(data[0], heapq.nlargest(2, [3, 9, 1, 7]), heapq.nsmallest(2, 'dbca'))
print(list(heapq.merge([1, 4, 9], [2, 3], [5])), heapq.heappushpop([1, 5], 3), heapq.heapreplace([1, 5], 3))
print(heapq.nlargest(2, ['aa', 'b', 'cccc'], key=len))

xs = [1, 3, 3, 7]
print(bisect.bisect_left(xs, 3), bisect.bisect_right(xs, 3), bisect.bisect(xs, 4))
bisect.insort(xs, 5)
bisect.insort_left(xs, 0)
print(xs, bisect.bisect_left(xs, 'x', key=str) if False else bisect.bisect_left([(1, 'a'), (2, 'b')], 2, key=lambda t: t[0]))
