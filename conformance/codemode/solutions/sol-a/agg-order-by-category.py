import asyncio
import json
from collections import defaultdict
from decimal import Decimal

order = json.loads(await get_order({"order_id": 1042}))
ids = sorted({i["product_id"] for i in order["items"]})
products = await asyncio.gather(*(get_product({"product_id": p}) for p in ids))
categories = {p["id"]: p["category"] for p in map(json.loads, products)}
totals = defaultdict(Decimal)
for item in order["items"]:
    totals[categories[item["product_id"]]] += item["qty"] * Decimal(str(item["unit_price"]))
await submit_answer({"subtotals": {c: float(round(v, 2)) for c, v in totals.items()}})
