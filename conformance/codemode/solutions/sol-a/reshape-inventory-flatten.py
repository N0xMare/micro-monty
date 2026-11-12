import csv
import io
import json

inventory = json.loads(await read_file({"path": "/data/inventory.json"}))
warehouses = inventory["warehouses"] if isinstance(inventory, dict) and "warehouses" in inventory else inventory
rows = []
if isinstance(warehouses, dict):
    entries = warehouses.items()
else:
    entries = ((w.get("name", w.get("warehouse", w.get("id"))), w) for w in warehouses)
for name, warehouse in entries:
    bins = warehouse["bins"] if isinstance(warehouse, dict) and "bins" in warehouse else warehouse
    for b in bins:
        rows.append((name, b["sku"], b["qty"], b["reorder_level"]))
out = io.StringIO(newline="")
writer = csv.writer(out, lineterminator="\n")
writer.writerow(["warehouse", "sku", "qty", "reorder_level"])
writer.writerows(sorted(rows, key=lambda r: (r[0], r[1])))
await write_file({"path": "/out/inventory_flat.csv", "content": out.getvalue()})
