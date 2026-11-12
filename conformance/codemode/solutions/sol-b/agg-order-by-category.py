import asyncio, json
from collections import defaultdict
order = json.loads(await get_order({"order_id": 1042}))
ids = sorted({i["product_id"] for i in order["items"]})
raw = await asyncio.gather(*(get_product({"product_id": p}) for p in ids))
categories = {p["id"]: p["category"] for p in map(json.loads, raw)}
subtotals = defaultdict(float)
for item in order["items"]:
    subtotals[categories[item["product_id"]]] += item["qty"] * item["unit_price"]
await submit_answer({"subtotals": {c: round(v, 2) for c, v in subtotals.items()}})
