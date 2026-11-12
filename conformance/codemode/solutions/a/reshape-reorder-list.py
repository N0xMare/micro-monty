import json

data = json.loads(await read_file({"path": "/data/inventory.json"}))

found = []

def walk(node, warehouse=None):
    if isinstance(node, dict):
        wh = node.get("warehouse", warehouse)
        if "qty" in node and "reorder_level" in node:
            found.append((wh, node))
            return
        for k, v in node.items():
            if k == "warehouse":
                continue
            # dict keyed by warehouse name
            child_wh = wh
            if isinstance(v, (list, dict)) and "warehouse" not in node and wh is None and k not in ("bins", "items", "warehouses", "inventory"):
                child_wh = k
            if k in ("bins", "items") and "name" in node and wh is None:
                child_wh = node["name"]
            walk(v, child_wh)
    elif isinstance(node, list):
        for item in node:
            walk(item, warehouse)

walk(data)
print(len(found), "bins found; sample:", found[:2])

reorder = []
for wh, b in found:
    qty, lvl = b["qty"], b["reorder_level"]
    if qty is None or lvl is None:
        continue
    if qty <= lvl:
        reorder.append({"warehouse": wh, "sku": b["sku"], "short_by": lvl - qty})

reorder.sort(key=lambda r: (-r["short_by"], str(r["warehouse"]), str(r["sku"])))
for r in reorder:
    print(r)
print(await submit_answer({"reorder": reorder}))
