import csv, io, json

data = json.loads(await read_file({"path": "/data/inventory.json"}))
print(json.dumps(data)[:800])

def warehouses(d):
    # Accept {"warehouses": [...]}, a list of warehouses, or {"<name>": {...}|[...]}.
    if isinstance(d, dict) and "warehouses" in d:
        d = d["warehouses"]
    if isinstance(d, list):
        for w in d:
            name = w.get("warehouse") or w.get("name") or w.get("id") or w.get("code")
            yield name, w
    else:
        for name, w in d.items():
            yield name, w

def bins(w):
    if isinstance(w, list):
        return w
    for k in ("bins", "items", "stock", "inventory", "skus"):
        if k in w:
            v = w[k]
            if isinstance(v, dict):
                return [{"sku": s, **b} for s, b in v.items()]
            return v
    raise KeyError(f"no bins in {list(w)}")

rows = []
for wname, w in warehouses(data):
    for b in bins(w):
        rows.append((str(wname), str(b["sku"]), b.get("qty", b.get("quantity")), b.get("reorder_level")))
rows.sort(key=lambda r: (r[0], r[1]))

buf = io.StringIO()
wr = csv.writer(buf, lineterminator="\n")
wr.writerow(["warehouse", "sku", "qty", "reorder_level"])
for r in rows:
    wr.writerow(["" if v is None else v for v in r])
content = buf.getvalue()
print(content)
print(await write_file({"path": "/out/inventory_flat.csv", "content": content}))
