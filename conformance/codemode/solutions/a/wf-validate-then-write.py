import json
from datetime import datetime, timezone

data = json.loads(await read_file({"path": "/data/inventory.json"}))

def parse_ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(timezone.utc)

# Normalize to a list of (name, record)
if isinstance(data, dict) and isinstance(data.get("warehouses"), list):
    items = data["warehouses"]
elif isinstance(data, list):
    items = data
else:
    src = data.get("warehouses", data) if isinstance(data, dict) else {}
    items = [dict(v, name=k) for k, v in src.items()]

warehouses = []
for w in items:
    name = w.get("name") or w.get("id") or w.get("warehouse")
    warehouses.append((name, w))

cutoff = datetime(2026, 3, 14, tzinfo=timezone.utc)
stale = [n for n, w in warehouses if parse_ts(w["updated"]) < cutoff]

if stale:
    print(json.dumps({"stale": stale}))
    await submit_answer({"written": False})
else:
    def total_qty(w):
        bins = w.get("bins", [])
        vals = bins.values() if isinstance(bins, dict) else bins
        return sum((b.get("qty") or 0) if isinstance(b, dict) else (b or 0) for b in vals)
    summary = {n: total_qty(w) for n, w in warehouses}
    await write_file({"path": "/out/summary.json", "content": json.dumps(summary, indent=2)})
    await submit_answer({"written": True})
