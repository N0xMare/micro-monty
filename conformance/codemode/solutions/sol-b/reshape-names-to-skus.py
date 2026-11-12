import asyncio, csv, io, json
from collections import Counter

sales_raw, products_raw = await asyncio.gather(read_file({"path": "/data/sales_2026q1.csv"}), list_products({}))
ids = {p["name"]: p["id"] for p in json.loads(products_raw)}
units = Counter()
for r in csv.DictReader(io.StringIO(sales_raw)):
    units[ids[r["product"]]] += int(r["units"])
await submit_answer({"units": dict(units)})
