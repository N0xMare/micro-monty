from typing import TypedDict


class Cart(TypedDict):
    items: int
    total: float


print(Cart(items=2, total=9.5))
Record = TypedDict('Record', {'total': int}, total=False)
print(sorted(Record.__optional_keys__), Record(total=1))
