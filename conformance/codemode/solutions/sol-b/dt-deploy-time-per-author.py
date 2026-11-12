import json
from collections import defaultdict
totals = defaultdict(int)
for d in json.loads(await list_deployments({})):
    totals[d["author"]] += d["duration_s"]
await submit_answer({"totals": {a: f"{s // 3600}:{s % 3600 // 60:02d}:{s % 60:02d}" for a, s in totals.items()}})
