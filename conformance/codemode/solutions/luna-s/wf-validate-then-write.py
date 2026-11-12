import json
from datetime import datetime, timezone

data = json.loads(await read_file({"path": "/data/inventory.json"}))
warehouses = data["warehouses"]
if isinstance(warehouses, dict):
    entries = list(warehouses.items())
else:
    entries = [(str(item.get("name", item.get("id", index))), item) for index, item in enumerate(warehouses)]
cutoff = datetime.fromisoformat("2026-03-14T00:00:00+00:00")
def timestamp(value):
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    return parsed.replace(tzinfo=timezone.utc) if parsed.tzinfo is None else parsed.astimezone(timezone.utc)
fresh = all(timestamp(warehouse["updated"]) >= cutoff for _, warehouse in entries)
if fresh:
    totals = {}
    for name, warehouse in entries:
        bins = warehouse["bins"]
        bins = bins.values() if isinstance(bins, dict) else bins
        totals[name] = sum(bin_item["qty"] for bin_item in bins)
    await write_file({"path": "/out/summary.json", "content": json.dumps(totals)})
await submit_answer({"written": fresh})
