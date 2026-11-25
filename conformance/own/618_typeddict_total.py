from typing import TypedDict


for total in (False, True):
    cls = TypedDict('Movie', {'title': str}, total=total)
    print(cls.__total__, sorted(cls.__required_keys__), sorted(cls.__optional_keys__))
    print(cls(title='Test'))
