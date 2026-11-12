import json
document = json.loads(await read_file({"path": "/data/inventory.json"}))
warehouses = document.get("warehouses", document) if isinstance(document, dict) else document
items = warehouses.items() if isinstance(warehouses, dict) else ((w.get("warehouse", w.get("name")), w) for w in warehouses)
reorder = []
for warehouse_name, warehouse in items:
    bins = warehouse.get("bins", []) if isinstance(warehouse, dict) else warehouse
    for item in bins:
        if item["qty"] <= item["reorder_level"]:
            reorder.append({"warehouse": warehouse_name, "sku": item["sku"], "short_by": item["reorder_level"] - item["qty"]})
reorder.sort(key=lambda x: (-x["short_by"], x["warehouse"], x["sku"]))
await submit_answer({"reorder": reorder})
