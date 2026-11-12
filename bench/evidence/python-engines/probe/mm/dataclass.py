from dataclasses import dataclass
@dataclass
class P:
    x: int
    y: int = 0
r = repr(P(1))
print(r)
