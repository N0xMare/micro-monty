import json

data = json.loads(await read_file({"path": "/data/inventory.json"}))
print(json.dumps(data)[:600])

GENERIC = {"warehouses", "bins", "items", "inventory", "stock", "locations", "data"}
found = []


def walk(node, wh):
    if isinstance(node, dict):
        if "qty" in node and "reorder_level" in node:
            w = node.get("warehouse", wh)
            found.append((w, node["sku"], node["qty"], node["reorder_level"]))
            return
        here = wh
        for k in ("warehouse", "name", "id", "code"):
            if isinstance(node.get(k), str):
                here = node[k]
                break
        for k, v in node.items():
            if isinstance(v, (dict, list)):
                walk(v, here if k in GENERIC or here != wh else k)
    elif isinstance(node, list):
        for item in node:
            walk(item, wh)


walk(data, None)
print(len(found), "bins")

reorder = [
    {"warehouse": w, "sku": sku, "short_by": lvl - qty}
    for w, sku, qty, lvl in found
    if qty <= lvl
]
reorder.sort(key=lambda r: (-r["short_by"], str(r["warehouse"]), str(r["sku"])))
print(reorder)
print(await submit_answer({"reorder": reorder}))
