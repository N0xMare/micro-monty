from typing import Annotated, NotRequired, Required, TypedDict

Base = TypedDict('Base', {'label': str, 'amount': Required[float]}, total=False)
Extra = TypedDict('Extra', {'extra': Annotated[NotRequired[int], 'metadata']})


class Cart(Base, Extra):
    count: int


assert Base.__required_keys__ == frozenset({'amount'})
assert Base.__optional_keys__ == frozenset({'label'})
assert Extra.__required_keys__ == frozenset()
assert Extra.__optional_keys__ == frozenset({'extra'})
assert Cart.__required_keys__ == frozenset({'amount', 'count'})
assert Cart.__optional_keys__ == frozenset({'label', 'extra'})
assert list(Cart.__annotations__) == ['label', 'amount', 'extra', 'count']
