class V:
    def __init__(s, x): s.x = x
    def __iadd__(s, o): s.x += o; return s
v = V(1)
v += 2
r = v.x
print(r)
