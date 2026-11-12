import asyncio
import csv
import io
from collections import Counter

text, products = await asyncio.gather(read_file({"path": "/data/sales_2026q1.csv"}), list_products({}))
ids = {p["name"]: p["id"] for p in products}
units = Counter()
for row in csv.DictReader(io.StringIO(text)):
    units[ids[row["product"]]] += int(row["units"])
await submit_answer({"units": dict(units)})
