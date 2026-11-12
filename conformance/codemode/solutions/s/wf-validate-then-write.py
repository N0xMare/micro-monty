import json
from datetime import datetime, timezone

raw = await read_file({"path": "/data/inventory.json"})
data = json.loads(raw)
print(json.dumps(data)[:1500])

# Normalize to a list of (name, warehouse_dict)
if isinstance(data, dict) and "warehouses" in data:
    whs = data["warehouses"]
else:
    whs = data
if isinstance(whs, dict):
    items = list(whs.items())
else:
    items = [(w.get("name") or w.get("id") or w.get("warehouse"), w) for w in whs]

def parse_ts(s):
    dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt

cutoff = datetime(2026, 3, 14, tzinfo=timezone.utc)
stale = [name for name, w in items if parse_ts(w["updated"]) < cutoff]
print("stale:", stale)

if stale:
    await submit_answer({"written": False})
else:
    summary = {}
    for name, w in items:
        bins = w.get("bins") or []
        if isinstance(bins, dict):
            bins = list(bins.values())
        summary[name] = sum((b.get("qty") or 0) if isinstance(b, dict) else b for b in bins)
    print(summary)
    await write_file({"path": "/out/summary.json", "content": json.dumps(summary, indent=2)})
    await submit_answer({"written": True})
