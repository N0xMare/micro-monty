import json

data = json.loads(await read_file({"path": "/data/inventory.json"}))
warehouses = data["warehouses"] if isinstance(data, dict) and "warehouses" in data else data
entries = warehouses.items() if isinstance(warehouses, dict) else ((w.get("name", w.get("warehouse", w.get("id"))), w) for w in warehouses)
reorder = []
for name, warehouse in entries:
    bins = warehouse["bins"] if isinstance(warehouse, dict) and "bins" in warehouse else warehouse
    for b in bins:
        if b["qty"] <= b["reorder_level"]:
            reorder.append({"warehouse": name, "sku": b["sku"], "short_by": b["reorder_level"] - b["qty"]})
reorder.sort(key=lambda r: (-r["short_by"], r["warehouse"], r["sku"]))
await submit_answer({"reorder": reorder})
