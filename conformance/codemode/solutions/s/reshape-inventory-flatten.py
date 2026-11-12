import csv, io, json

data = json.loads(await read_file({"path": "/data/inventory.json"}))
print(json.dumps(data, indent=1)[:800])

def wh_name(w, fallback=None):
    for k in ("warehouse", "name", "id", "code"):
        if k in w and not isinstance(w[k], (list, dict)):
            return str(w[k])
    return fallback

def bins_of(w):
    for k in ("bins", "items", "stock", "inventory", "skus"):
        if k in w:
            b = w[k]
            if isinstance(b, dict):  # sku -> {qty, reorder_level}
                return [{"sku": s, **v} for s, v in b.items()]
            return b
    raise KeyError(f"no bins in {list(w)}")

if isinstance(data, dict) and "warehouses" in data:
    whs = data["warehouses"]
else:
    whs = data

if isinstance(whs, dict):  # name -> warehouse
    pairs = [(name, w if isinstance(w, dict) else {"bins": w}) for name, w in whs.items()]
    pairs = [(wh_name(w, name) if "bins" not in w or name is None else name, w) for name, w in pairs]
else:
    pairs = [(wh_name(w), w) for w in whs]

rows = []
for name, w in pairs:
    for b in bins_of(w):
        rows.append((name, str(b["sku"]), b.get("qty", b.get("quantity")), b.get("reorder_level")))

rows.sort(key=lambda r: (r[0], r[1]))
buf = io.StringIO()
writer = csv.writer(buf, lineterminator="\n")
writer.writerow(["warehouse", "sku", "qty", "reorder_level"])
writer.writerows(["" if v is None else v for v in r] for r in rows)
content = buf.getvalue()
print(content)

res = await write_file({"path": "/out/inventory_flat.csv", "content": content})
print(res)
