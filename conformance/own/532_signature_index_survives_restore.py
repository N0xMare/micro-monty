# Named tuples and dataclasses share one signature code per parameter
# list (`_mm.signature`'s index, at most 4,096 per session). A snapshot
# restored in another process keeps that index, so the restored session
# reuses the codes the one that went on reuses and counts toward the cap
# from the same point: the durability gate's restore-against-snapshot
# comparison checks the fuel and final state of this case. Before, a
# restore started the index empty and built every signature again (more
# fuel, more codes; 5,000 distinct ones ended past the cap only when a
# restore came in between).
from collections import namedtuple
from dataclasses import dataclass

kinds = []
for i in range(3000):
    kinds.append(namedtuple('N', [f'f{i}', 'shared']))
# The same parameter lists again: each one reuses its code.
again = [namedtuple('M', [f'f{i}', 'shared']) for i in range(0, 3000, 7)]
print(len(kinds), len(again), kinds[2999](1, 2), again[-1](3, 4))


@dataclass
class P:
    x: int
    y: int = 0


@dataclass
class Q:
    x: int
    y: int = 0


print(P(1), Q(2, 3), P(1) == P(1, 0))
