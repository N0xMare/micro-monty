import json
from datetime import datetime, timezone
inventory = json.loads(await read_file({"path": "/data/inventory.json"}))
warehouses = inventory.get("warehouses", inventory) if isinstance(inventory, dict) else inventory
if isinstance(warehouses, dict):
    entries = list(warehouses.items())
else:
    entries = [(w.get("warehouse", w.get("name", w.get("id"))), w) for w in warehouses]
cutoff = datetime(2026, 3, 14, tzinfo=timezone.utc)
stale = any(datetime.fromisoformat(w["updated"].replace("Z", "+00:00")) < cutoff for name, w in entries)
if stale:
    await submit_answer({"written": False})
else:
    summary = {}
    for name, warehouse in entries:
        bins = warehouse["bins"]
        if isinstance(bins, dict):
            bins = bins.values()
        summary[name] = sum(b["qty"] for b in bins)
    await write_file({"path": "/out/summary.json", "content": json.dumps(summary)})
    await submit_answer({"written": True})
