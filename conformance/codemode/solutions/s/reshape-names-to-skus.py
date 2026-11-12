import asyncio, csv, io
from collections import defaultdict

text, products = await asyncio.gather(
    read_file({"path": "/data/sales_2026q1.csv"}),
    list_products({}),
)

def norm(s):
    return " ".join((s or "").split()).casefold()

name_to_id = {norm(p["name"]): p["id"] for p in products}

rows = list(csv.DictReader(io.StringIO(text.lstrip("\ufeff"))))
rows = [{norm(k): (v or "").strip() for k, v in r.items() if k is not None} for r in rows]
cols = list(rows[0].keys()) if rows else []
print("columns:", cols)

def pick(cands):
    for c in cands:
        if c in cols:
            return c
    for c in cols:
        if any(x in c for x in cands):
            return c
    return None

name_col = pick(["product_name", "product", "name", "item"])
units_col = pick(["units", "units_sold", "quantity", "qty"])
print("using", name_col, units_col)

units = defaultdict(int)
unmatched = defaultdict(int)
for r in rows:
    nm = r.get(name_col, "")
    if not nm:
        continue
    raw = r.get(units_col, "").replace(",", "")
    if not raw:
        continue
    q = int(float(raw))
    pid = name_to_id.get(norm(nm))
    if pid is None:
        unmatched[nm] += q
    else:
        units[pid] += q

if unmatched:
    print("UNMATCHED names:", dict(unmatched))
units = dict(sorted(units.items()))
print(units)
print(await submit_answer({"units": units}))
