import json
from datetime import datetime, timezone

raw = await read_file({"path": "/data/inventory.json"})
data = json.loads(raw)
warehouses = data["warehouses"] if isinstance(data, dict) and "warehouses" in data else data
entries = warehouses.items() if isinstance(warehouses, dict) else ((w.get("name", w.get("warehouse")), w) for w in warehouses)
totals = {}
for name, warehouse in entries:
    updated = datetime.fromisoformat(warehouse["updated"].replace("Z", "+00:00"))
    if updated.tzinfo is None: updated = updated.replace(tzinfo=timezone.utc)
    if updated < datetime(2026, 3, 14, tzinfo=timezone.utc):
        await submit_answer({"written": False})
        break
    totals[name] = sum(bin_["qty"] for bin_ in warehouse["bins"])
else:
    await write_file({"path": "/out/summary.json", "content": json.dumps(totals)})
    await submit_answer({"written": True})
