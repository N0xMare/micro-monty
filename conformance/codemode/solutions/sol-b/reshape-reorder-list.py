import json

inventory = json.loads(await read_file({"path": "/data/inventory.json"}))
reorder = []
def collect(node, warehouse=""):
    if isinstance(node, list):
        for child in node:
            collect(child, warehouse)
    elif isinstance(node, dict):
        if all(k in node for k in ("sku", "qty", "reorder_level")):
            if node["qty"] <= node["reorder_level"]:
                reorder.append({"warehouse": node.get("warehouse", warehouse), "sku": node["sku"], "short_by": node["reorder_level"] - node["qty"]})
        else:
            label = node.get("warehouse", node.get("name", node.get("id", warehouse)))
            for key, value in node.items():
                if isinstance(value, (dict, list)):
                    collect(value, label if label else (key if key not in ("warehouses", "bins", "inventory") else warehouse))
collect(inventory)
reorder.sort(key=lambda r: (-r["short_by"], r["warehouse"], r["sku"]))
await submit_answer({"reorder": reorder})
