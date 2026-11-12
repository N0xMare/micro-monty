import json, csv, io

inventory = json.loads(await read_file({"path": "/data/inventory.json"}))
rows = []
def collect(node, warehouse=""):
    if isinstance(node, list):
        for child in node:
            collect(child, warehouse)
    elif isinstance(node, dict):
        if all(k in node for k in ("sku", "qty", "reorder_level")):
            rows.append((node.get("warehouse", warehouse), node["sku"], node["qty"], node["reorder_level"]))
        else:
            label = node.get("warehouse", node.get("name", node.get("id", warehouse)))
            for key, value in node.items():
                if isinstance(value, (dict, list)):
                    collect(value, label if label else (key if key not in ("warehouses", "bins", "inventory") else warehouse))
collect(inventory)
out = io.StringIO(newline="")
writer = csv.writer(out, lineterminator="\n")
writer.writerow(["warehouse", "sku", "qty", "reorder_level"])
writer.writerows(sorted(rows, key=lambda r: (r[0], r[1])))
await write_file({"path": "/out/inventory_flat.csv", "content": out.getvalue()})
