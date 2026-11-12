import asyncio, csv, io, json
from collections import defaultdict

sales_raw, products_raw = await asyncio.gather(read_file({"path": "/data/sales_2026q1.csv"}), list_products({}))
name_to_id = {product["name"]: product["id"] for product in json.loads(products_raw)}
units = defaultdict(int)
for row in csv.DictReader(io.StringIO(sales_raw)):
    units[name_to_id[row["product"]]] += int(row["units"])
await submit_answer({"units": dict(units)})
