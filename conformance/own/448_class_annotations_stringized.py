# Class-body annotations stringized as under `from __future__ import
# annotations` (micro-monty never evaluates them): every expression kind
# the unparser rewrites, with CPython's precedence-driven parentheses.
from __future__ import annotations
class A:
    a: list[int]
    b: dict[str, int | None]
    c: tuple[int, ...]
    d: x[1:2, ::3]
    e: (1, 2)
    f: ()
    g: (1,)
    h: [int, str]
    i: {1, 2}
    j: {1: 2, **m}
    k: f(1, *a, b=2, **c)
    l: a < b <= c > d >= e == f != g in h not in i is j is not k
    m: not a and b or c
    n: -a + ~b * +c ** d // e % f @ g / h
    o: a | b ^ c & d << e >> f
    p: 'str' 'cat'
    q: b'bytes'
    r: f'{x!r:>{w}}'
    s: a if b else c
    t: lambda x: x
    v: a.b.c[d]
    x: 1.5
    y: ...
    z: None
    aa: (yield_ for yield_ in b)
    ab: [i for i in c if i]
    ac: {i: j for i, j in d}
    ad: {i for i in d}
    af: x[*a]
    ag: x[a:b]
    ah: x[:]
    ai: x[::]
    aj: {}
    ak: 1j
    al: -1
    am: (-1) ** 2
    an: a[b](c).d
for k, v in A.__annotations__.items():
    print(k, repr(v))
