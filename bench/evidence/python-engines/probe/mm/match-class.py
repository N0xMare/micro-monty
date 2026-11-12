class P:
    __match_args__ = ("x",)
    def __init__(s, x): s.x = x
match P(3):
    case P(x=3): r = "matched"
    case _: r = "no"
print(r)
