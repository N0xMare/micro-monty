import csv, io, json

data = json.loads(await read_file({"path": "/data/inventory.json"}))
print("top-level:", type(data).__name__, list(data)[:10] if isinstance(data, dict) else len(data))

def bins_of(obj):
    for k in ("bins", "items", "stock", "inventory", "skus"):
        if isinstance(obj, dict) and k in obj:
            return obj[k]
    return obj

def rows_from_bins(warehouse, bins):
    if isinstance(bins, dict):  # sku -> {qty, reorder_level}
        bins = [dict(v, sku=k) if "sku" not in v else v for k, v in bins.items()]
    for b in bins:
        yield {"warehouse": warehouse, "sku": b["sku"], "qty": b["qty"], "reorder_level": b["reorder_level"]}

rows = []
whs = data.get("warehouses", data) if isinstance(data, dict) else data
if isinstance(whs, dict):  # warehouse name -> bins
    for name, w in whs.items():
        rows += rows_from_bins(name, bins_of(w))
else:
    for w in whs:
        name = next(w[k] for k in ("warehouse", "name", "id", "code") if k in w)
        rows += rows_from_bins(name, bins_of(w))

rows.sort(key=lambda r: (str(r["warehouse"]), str(r["sku"])))
buf = io.StringIO()
writer = csv.DictWriter(buf, fieldnames=["warehouse", "sku", "qty", "reorder_level"], lineterminator="\n")
writer.writeheader()
writer.writerows(rows)
content = buf.getvalue()

print(content)
print(await write_file({"path": "/out/inventory_flat.csv", "content": content}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"path": "/out/inventory_flat.csv", "rows": len(rows)}))
