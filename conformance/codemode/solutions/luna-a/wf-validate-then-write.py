import json
from datetime import datetime, timezone

data = json.loads(await read_file({"path": "/data/inventory.json"}))
warehouses = data.get("warehouses", data)
items = warehouses.items() if isinstance(warehouses, dict) else ((w.get("name", w.get("warehouse")), w) for w in warehouses)
entries = list(items)
threshold = datetime(2026, 3, 14, tzinfo=timezone.utc)
def parse_timestamp(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00"))

if any(parse_timestamp(warehouse["updated"]) < threshold for _, warehouse in entries):
    await submit_answer({"written": False})
else:
    totals = {name: sum(int(bin_item["qty"]) for bin_item in warehouse["bins"]) for name, warehouse in entries}
    await write_file({"path": "/out/summary.json", "content": json.dumps(totals)})
    await submit_answer({"written": True})
