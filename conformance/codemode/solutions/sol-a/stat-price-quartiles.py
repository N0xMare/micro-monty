import json
import math
products = json.loads(await list_products({}))
values = sorted(product["price"] for product in products)
quartiles = {}
for k in [1, 2, 3]:
    position = (len(values) - 1) * k / 4
    lo, hi = math.floor(position), math.ceil(position)
    quartiles[f"q{k}"] = round(values[lo] + (position - lo) * (values[hi] - values[lo]), 2)
await submit_answer(quartiles)
