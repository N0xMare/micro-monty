import asyncio
from collections import defaultdict
from decimal import Decimal

order = await get_order({"order_id": 1042})
ids = sorted({i["product_id"] for i in order["items"]})
products = await asyncio.gather(*(get_product({"product_id": pid}) for pid in ids))
categories = {p["id"]: p["category"] for p in products}
subtotals = defaultdict(Decimal)
for item in order["items"]:
    subtotals[categories[item["product_id"]]] += item["qty"] * Decimal(str(item["unit_price"]))
await submit_answer({"subtotals": {c: float(round(v, 2)) for c, v in subtotals.items()}})
