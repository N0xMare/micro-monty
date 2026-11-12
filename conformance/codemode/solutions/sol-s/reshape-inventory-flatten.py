import csv
import io
import json

inventory = json.loads(await read_file({"path": "/data/inventory.json"}))
rows = []
warehouses = inventory["warehouses"]
if isinstance(warehouses, dict):
    warehouses = [dict(w, name=name) for name, w in warehouses.items()]
for warehouse in warehouses:
    name = warehouse.get("name", warehouse.get("id", warehouse.get("warehouse")))
    for bin in warehouse["bins"]:
        rows.append([name, bin["sku"], bin["qty"], bin["reorder_level"]])
rows.sort(key=lambda r: (r[0], r[1]))
output = io.StringIO(newline="")
writer = csv.writer(output, lineterminator="\n")
writer.writerow(["warehouse", "sku", "qty", "reorder_level"])
writer.writerows(rows)
await write_file({"path": "/out/inventory_flat.csv", "content": output.getvalue()})
