import asyncio
import csv
import io
import json
from collections import defaultdict

text, products_raw = await asyncio.gather(
    read_file({"path": "/data/sales_2026q1.csv"}),
    list_products({}),
)
products = json.loads(products_raw)


def norm(s):
    return " ".join(str(s).split()).casefold()


name_to_id = {norm(p["name"]): p["id"] for p in products}

units = defaultdict(int)
unmatched = defaultdict(int)
for row in csv.DictReader(io.StringIO(text)):
    row = {(k or "").strip(): (v or "").strip() for k, v in row.items()}
    name = row.get("product", "")
    if not name:
        continue
    qty = int(float(row.get("units") or 0))
    pid = name_to_id.get(norm(name))
    if pid is None:
        unmatched[name] += qty
        continue
    units[pid] += qty

if unmatched:
    print("unmatched product names:", dict(unmatched))
result = dict(sorted(units.items()))
print(result)
print(await submit_answer({"units": result}))
