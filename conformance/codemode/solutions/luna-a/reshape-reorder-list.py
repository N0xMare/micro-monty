import json

data = json.loads(await read_file({"path": "/data/inventory.json"}))
reorder = []
for warehouse in data["warehouses"]:
    warehouse_name = warehouse.get("warehouse", warehouse.get("name"))
    for item in warehouse["bins"]:
        if item["qty"] <= item["reorder_level"]:
            reorder.append({"warehouse": warehouse_name, "sku": item["sku"], "short_by": item["reorder_level"] - item["qty"]})
reorder.sort(key=lambda item: (-item["short_by"], item["warehouse"], item["sku"]))
await submit_answer({"reorder": reorder})
