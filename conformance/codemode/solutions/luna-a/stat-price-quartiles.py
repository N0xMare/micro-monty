import json
import math

products = json.loads(await list_products())
values = sorted(float(p["price"]) for p in products)
def quartile(k):
    position = (len(values) - 1) * k / 4
    low = math.floor(position)
    high = math.ceil(position)
    return values[low] + (values[high] - values[low]) * (position - low)
await submit_answer({"q1": round(quartile(1), 2), "q2": quartile(2), "q3": quartile(3)})
