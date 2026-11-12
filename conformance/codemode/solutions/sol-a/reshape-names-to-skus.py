import asyncio
import csv
import io
import json
from collections import Counter

text, catalogue = await asyncio.gather(read_file({"path": "/data/sales_2026q1.csv"}), list_products({}))
ids = {p["name"]: p["id"] for p in json.loads(catalogue)}
units = Counter()
for row in csv.DictReader(io.StringIO(text)):
    name = row["product"] if "product" in row else row["product_name"]
    units[ids[name]] += int(row["units"])
await submit_answer({"units": dict(units)})
