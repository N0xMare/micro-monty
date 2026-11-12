import json
import math

rows = json.loads(await list_products({}))
values = sorted(p["price"] for p in rows)
def quartile(k):
    pos = (len(values) - 1) * k / 4
    lo = math.floor(pos); hi = math.ceil(pos)
    return values[lo] + (values[hi] - values[lo]) * (pos - lo)
await submit_answer({"q1": round(quartile(1), 2), "q2": round(quartile(2), 2), "q3": round(quartile(3), 2)})
