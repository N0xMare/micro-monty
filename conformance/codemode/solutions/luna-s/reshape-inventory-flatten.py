import csv
import io
import json
document = json.loads(await read_file({"path": "/data/inventory.json"}))
warehouses = document.get("warehouses", document) if isinstance(document, dict) else document
entries = []
items = warehouses.items() if isinstance(warehouses, dict) else ((w.get("warehouse", w.get("name")), w) for w in warehouses)
for warehouse_name, warehouse in items:
    bins = warehouse.get("bins", []) if isinstance(warehouse, dict) else warehouse
    for item in bins:
        entries.append([warehouse_name, item["sku"], item["qty"], item["reorder_level"]])
entries.sort(key=lambda row: (row[0], row[1]))
out = io.StringIO(newline="")
writer = csv.writer(out, lineterminator="\n")
writer.writerow(["warehouse", "sku", "qty", "reorder_level"])
writer.writerows(entries)
await write_file({"path": "/out/inventory_flat.csv", "content": out.getvalue()})
