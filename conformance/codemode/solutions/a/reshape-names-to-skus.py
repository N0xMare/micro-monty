import csv, io, json
from collections import defaultdict

text = await read_file({"path": "/data/sales_2026q1.csv"})
products = json.loads(await list_products({}))

def norm(s):
    return " ".join((s or "").split()).casefold()

by_name = {norm(p["name"]): p["id"] for p in products}

rows = list(csv.DictReader(io.StringIO(text)))
print("columns:", list(rows[0].keys()) if rows else None, "rows:", len(rows))

def col(row, *cands):
    for k in row:
        if k and k.strip().lower() in cands:
            return row[k]
    return None

units = defaultdict(int)
unmatched = defaultdict(int)
for r in rows:
    name = col(r, "product", "product_name", "name", "item")
    q = col(r, "units", "quantity", "qty", "units_sold")
    qty = int(float(q.strip())) if q and q.strip() else 0
    pid = by_name.get(norm(name))
    if pid is None:
        unmatched[name] += qty
    else:
        units[pid] += qty

print("unmatched names:", dict(unmatched))
print(dict(units))
print(await submit_answer({"units": dict(sorted(units.items()))}))
