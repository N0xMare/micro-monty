# repr of builtin containers past the native walk's slice goes on in the
# prelude twin, which renders every shape as the walk does (durability's
# small fuel slices make the twin render each level here).
class L(list):
    pass


class D(dict):
    pass


class S(set):
    pass


class R:
    def __repr__(self):
        return "R!"


me = []
me.append(me)
d = {"self": None}
d["self"] = d
nested = 7
for _ in range(300):
    nested = {"k": [nested, (nested,)]} if isinstance(nested, int) else {"k": [nested]}
shapes = [
    [], (), (1,), (1, 2), set(), frozenset(), {}, {1: 2}, {3}, frozenset({4}),
    L([1, L([2])]), D(a=D(b=1)), S(), S({5}), [R(), (R(),), {"r": R()}],
    {}.keys(), {1: 2}.values(), {1: (2, 3)}.items(), me, d, [me, d, me],
    [[[[[[[[[[1]]]]]]]]]], ((((((((((2,),),),),),),),),),), "x'y\"z", b"\x00", 1.5, -3, None,
    {"a": {"b": {"c": [1, {"d": (2, frozenset({3}))}]}}},
]
for x in shapes:
    print(repr(x))
print(len(repr(nested)), repr(nested)[:40], repr(nested)[-40:])
big = [shapes[i % len(shapes)] for i in range(40_000)]
r = repr(big)
print(len(r), r[:60], r[-60:], hash(r) == hash(repr(big)))
deep = 1
for _ in range(400):
    deep = [deep, {"x": deep}] if False else [deep]
print(repr(deep) == "[" * 400 + "1" + "]" * 400)
