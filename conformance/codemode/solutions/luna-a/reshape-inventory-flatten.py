import csv
import io
import json

data = json.loads(await read_file({"path": "/data/inventory.json"}))
rows = []
for warehouse in data["warehouses"]:
    warehouse_name = warehouse.get("warehouse", warehouse.get("name"))
    for bin_item in warehouse["bins"]:
        rows.append([warehouse_name, bin_item["sku"], bin_item["qty"], bin_item["reorder_level"]])
rows.sort(key=lambda row: (row[0], row[1]))
output = io.StringIO(newline="")
writer = csv.writer(output, lineterminator="\n")
writer.writerow(["warehouse", "sku", "qty", "reorder_level"])
writer.writerows(rows)
await write_file({"path": "/out/inventory_flat.csv", "content": output.getvalue()})
