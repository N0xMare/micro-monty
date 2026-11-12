import json

inventory = json.loads(await read_file({"path": "/data/inventory.json"}))
warehouses = inventory["warehouses"]
if isinstance(warehouses, dict):
    warehouses = [dict(w, name=name) for name, w in warehouses.items()]
reorder = []
for warehouse in warehouses:
    name = warehouse.get("name", warehouse.get("id", warehouse.get("warehouse")))
    for bin in warehouse["bins"]:
        if bin["qty"] <= bin["reorder_level"]:
            reorder.append({"warehouse": name, "sku": bin["sku"], "short_by": bin["reorder_level"] - bin["qty"]})
reorder.sort(key=lambda r: (-r["short_by"], r["warehouse"], r["sku"]))
await submit_answer({"reorder": reorder})
