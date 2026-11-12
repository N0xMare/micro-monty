import json
from datetime import datetime, timezone

data = json.loads(await read_file({"path": "/data/inventory.json"}))
print(json.dumps(data)[:1500])

# Accept {"warehouses": [...]} / {"warehouses": {...}}, a bare list, or a name -> warehouse mapping.
wh = data.get("warehouses", data) if isinstance(data, dict) else data
if isinstance(wh, dict):
    items = list(wh.items())
else:
    items = [(w.get("name") or w.get("id") or w.get("warehouse"), w) for w in wh]

def parse(ts):
    return datetime.fromisoformat(ts.replace("Z", "+00:00"))

cutoff = datetime(2026, 3, 14, tzinfo=timezone.utc)
stale = [name for name, w in items if parse(w["updated"]) < cutoff]
print("stale:", stale)

if stale:
    print(await submit_answer({"written": False}))
else:
    summary = {}
    for name, w in items:
        bins = w["bins"]
        bins = bins.values() if isinstance(bins, dict) else bins
        summary[name] = sum(b["qty"] if isinstance(b, dict) else b for b in bins)
    print(summary)
    print(await write_file({"path": "/out/summary.json", "content": json.dumps(summary, indent=2)}))
    print(await submit_answer({"written": True}))
