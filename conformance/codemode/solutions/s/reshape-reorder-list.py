import json

data = json.loads(await read_file({"path": "/data/inventory.json"}))
print(json.dumps(data, indent=1)[:1500])

found = []

def walk(node, warehouse=None, key=None):
    if isinstance(node, dict):
        wh = node.get("warehouse") or node.get("warehouse_id") or warehouse
        if wh is None and key is not None and any(isinstance(v, (list, dict)) for v in node.values()) and "qty" not in node:
            wh = key
        if "qty" in node and "reorder_level" in node:
            sku = node.get("sku") or (key if key is not None and not isinstance(key, int) else None)
            found.append({"warehouse": wh, "sku": sku, "qty": node["qty"], "reorder_level": node["reorder_level"]})
            return
        for k, v in node.items():
            if isinstance(v, (dict, list)):
                # a nested warehouse name given by its key, e.g. {"warehouses": {"WH1": {...}}}
                next_wh = wh
                if isinstance(v, dict) and k not in ("warehouses", "bins", "items", "inventory") and ("name" in node or "bins" in v or "items" in v):
                    pass
                walk(v, wh if wh is not None else (k if k not in ("warehouses", "bins", "items", "inventory", "stock") else None), k)
    elif isinstance(node, list):
        for item in node:
            walk(item, warehouse, None)

walk(data)
# Prefer explicit warehouse names if the warehouse object uses "name"
reorder = []
for f in found:
    if f["qty"] is None or f["reorder_level"] is None:
        continue
    if f["qty"] <= f["reorder_level"]:
        reorder.append({"warehouse": str(f["warehouse"]), "sku": str(f["sku"]), "short_by": f["reorder_level"] - f["qty"]})

reorder.sort(key=lambda r: (-r["short_by"], r["warehouse"], r["sku"]))
print(len(found), "bins scanned")
print(reorder)
if any(r["warehouse"] == "None" or r["sku"] == "None" for r in reorder):
    print("WARNING: could not determine warehouse/sku for some bins; not submitting")
else:
    print(await submit_answer({"reorder": reorder}))
