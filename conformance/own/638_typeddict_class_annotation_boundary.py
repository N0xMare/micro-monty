from typing import Annotated, NotRequired, Required, TypedDict

Base = TypedDict('Base', {'id': Required[int], 'label': NotRequired[str]}, total=False)


class Extra(Base, total=False):
    Required = int
    note: Annotated[str, 'NotRequired']
    score: Required


assert Extra.__required_keys__ == frozenset({'id'})
assert Extra.__optional_keys__ == frozenset({'label', 'note', 'score'})
assert Extra(id=1, score=3) == {'id': 1, 'score': 3}
