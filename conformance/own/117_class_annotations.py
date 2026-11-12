# Class-body annotations are stored stringized, as with the future import.
from __future__ import annotations


class A:
    """Doc."""
    x: int
    y: list[dict[str, 'A']] = []
    z: int | None = None
    w: tuple[int, ...]
    if True:
        nested: 'str'

    def method(self) -> None:
        local: int = 1
        return local


class B(A):
    pass


print(A.__annotations__, A.__doc__)
print(B.__annotations__, A().method())
print(type(A.__annotations__).__name__, 'x' in A.__dict__)
